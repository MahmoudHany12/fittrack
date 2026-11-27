//! External food data providers (Open Food Facts, USDA FoodData Central).
//!
//! Providers sit behind the [`FoodProvider`] trait so tests can inject fakes and
//! never touch the network. A failing provider only logs a warning.

use async_trait::async_trait;
use serde_json::Value;
use std::{sync::Arc, time::Duration};
use tokio::task::JoinSet;

const USER_AGENT: &str = "FitTrack/0.1 (open-source)";
const TIMEOUT: Duration = Duration::from_secs(5);
const MAX_PER_PROVIDER: usize = 15;

/// A food as reported by an external provider; nutrients are per `serving_g`.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalFood {
    /// `"usda"` or `"off"`.
    pub source: String,
    pub external_id: String,
    pub name: String,
    pub brand: Option<String>,
    pub kcal: f64,
    pub protein_g: f64,
    pub carbs_g: f64,
    pub fat_g: f64,
    pub serving_g: f64,
}

impl ExternalFood {
    /// Drops junk from untrusted provider data: returns `None` if unusable.
    pub fn sanitized(mut self) -> Option<Self> {
        if self.source != "usda" && self.source != "off" {
            return None;
        }
        let vals = [
            self.kcal,
            self.protein_g,
            self.carbs_g,
            self.fat_g,
            self.serving_g,
        ];
        if vals.iter().any(|v| !v.is_finite() || *v < 0.0) || self.serving_g <= 0.0 {
            return None;
        }
        self.name = self.name.trim().chars().take(200).collect();
        self.brand = self
            .brand
            .map(|b| b.trim().chars().take(200).collect::<String>())
            .filter(|b| !b.is_empty());
        self.external_id = self.external_id.trim().chars().take(100).collect();
        if self.name.is_empty() || self.external_id.is_empty() {
            return None;
        }
        // Same bounds we enforce on custom foods.
        if self.kcal > 5000.0
            || self.protein_g > 1000.0
            || self.carbs_g > 1000.0
            || self.fat_g > 1000.0
        {
            return None;
        }
        Some(self)
    }
}

#[async_trait]
pub trait FoodProvider: Send + Sync {
    fn name(&self) -> &'static str;
    async fn search(&self, query: &str) -> anyhow::Result<Vec<ExternalFood>>;
}

/// The set of providers used by `GET /foods/search`. Inject in tests with
/// `app(state).layer(Extension(FoodProviders::new(vec![...])))`.
#[derive(Clone)]
pub struct FoodProviders(Arc<Vec<Arc<dyn FoodProvider>>>);

impl FoodProviders {
    pub fn new(providers: Vec<Arc<dyn FoodProvider>>) -> Self {
        Self(Arc::new(providers))
    }

    /// Open Food Facts always; USDA only when `USDA_API_KEY` is set.
    pub fn from_env() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(TIMEOUT)
            .connect_timeout(TIMEOUT)
            .build()
            .expect("http client");
        let mut list: Vec<Arc<dyn FoodProvider>> = vec![Arc::new(OpenFoodFacts {
            client: client.clone(),
        })];
        match std::env::var("USDA_API_KEY") {
            Ok(key) if !key.trim().is_empty() => list.push(Arc::new(Usda {
                client,
                api_key: key.trim().to_string(),
            })),
            _ => tracing::info!("USDA_API_KEY not set; USDA food search disabled"),
        }
        Self::new(list)
    }

    /// Query all providers concurrently. Errors and timeouts are logged and skipped.
    pub async fn search_all(&self, query: &str) -> Vec<ExternalFood> {
        let mut set = JoinSet::new();
        for (i, p) in self.0.iter().enumerate() {
            let p = p.clone();
            let q = query.to_string();
            set.spawn(async move {
                // Hard cap on top of the HTTP client's own timeouts.
                let res =
                    tokio::time::timeout(TIMEOUT + Duration::from_secs(1), p.search(&q)).await;
                let foods = match res {
                    Ok(Ok(f)) => f,
                    Ok(Err(e)) => {
                        tracing::warn!(provider = p.name(), error = %e, "food provider failed");
                        Vec::new()
                    }
                    Err(_) => {
                        tracing::warn!(provider = p.name(), "food provider timed out");
                        Vec::new()
                    }
                };
                (i, foods)
            });
        }
        let mut parts: Vec<(usize, Vec<ExternalFood>)> = Vec::new();
        while let Some(joined) = set.join_next().await {
            if let Ok(part) = joined {
                parts.push(part);
            }
        }
        parts.sort_by_key(|(i, _)| *i);
        parts
            .into_iter()
            .flat_map(|(_, foods)| foods)
            .filter_map(ExternalFood::sanitized)
            .collect()
    }
}

// ---------- Open Food Facts (ODbL) ----------

pub struct OpenFoodFacts {
    client: reqwest::Client,
}

#[async_trait]
impl FoodProvider for OpenFoodFacts {
    fn name(&self) -> &'static str {
        "off"
    }

    async fn search(&self, query: &str) -> anyhow::Result<Vec<ExternalFood>> {
        let body: Value = self
            .client
            .get("https://world.openfoodfacts.org/cgi/search.pl")
            .query(&[
                ("search_terms", query),
                ("search_simple", "1"),
                ("action", "process"),
                ("json", "1"),
                ("page_size", "15"),
                (
                    "fields",
                    "code,product_name,brands,nutriments,serving_quantity",
                ),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(parse_off(&body))
    }
}

fn num(v: Option<&Value>) -> Option<f64> {
    match v? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub fn parse_off(body: &Value) -> Vec<ExternalFood> {
    let Some(products) = body.get("products").and_then(Value::as_array) else {
        return Vec::new();
    };
    products
        .iter()
        .filter_map(|p| {
            let name = p.get("product_name")?.as_str()?.trim().to_string();
            let code = p.get("code")?.as_str()?.to_string();
            let n = p.get("nutriments")?;
            let kcal100 = num(n.get("energy-kcal_100g"))?;
            let serving = num(p.get("serving_quantity"))
                .filter(|s| *s > 0.0)
                .unwrap_or(100.0);
            let k = serving / 100.0;
            let macro_of = |key: &str| round1(num(n.get(key)).unwrap_or(0.0) * k);
            let brand = p
                .get("brands")
                .and_then(Value::as_str)
                .and_then(|b| b.split(',').next())
                .map(|b| b.trim().to_string());
            Some(ExternalFood {
                source: "off".into(),
                external_id: code,
                name,
                brand,
                kcal: round1(kcal100 * k),
                protein_g: macro_of("proteins_100g"),
                carbs_g: macro_of("carbohydrates_100g"),
                fat_g: macro_of("fat_100g"),
                serving_g: serving,
            })
        })
        .take(MAX_PER_PROVIDER)
        .collect()
}

// ---------- USDA FoodData Central (public domain) ----------

pub struct Usda {
    client: reqwest::Client,
    api_key: String,
}

#[async_trait]
impl FoodProvider for Usda {
    fn name(&self) -> &'static str {
        "usda"
    }

    async fn search(&self, query: &str) -> anyhow::Result<Vec<ExternalFood>> {
        // Key goes in a header so it can never leak through error URLs/logs.
        let body: Value = self
            .client
            .get("https://api.nal.usda.gov/fdc/v1/foods/search")
            .header("X-Api-Key", &self.api_key)
            .query(&[("query", query), ("pageSize", "15")])
            .send()
            .await
            .map_err(|e| e.without_url())?
            .error_for_status()
            .map_err(|e| e.without_url())?
            .json()
            .await
            .map_err(|e| e.without_url())?;
        Ok(parse_usda(&body))
    }
}

pub fn parse_usda(body: &Value) -> Vec<ExternalFood> {
    let Some(foods) = body.get("foods").and_then(Value::as_array) else {
        return Vec::new();
    };
    foods
        .iter()
        .filter_map(|f| {
            let id = f.get("fdcId")?.as_i64()?.to_string();
            let name = f.get("description")?.as_str()?.trim().to_string();
            let nutrients = f.get("foodNutrients")?.as_array()?;
            // value per 100 g, matched by nutrient number or id
            let find = |number: &str, id: i64| -> Option<f64> {
                nutrients.iter().find_map(|n| {
                    let by_number = n.get("nutrientNumber").and_then(Value::as_str) == Some(number);
                    let by_id = n.get("nutrientId").and_then(Value::as_i64) == Some(id);
                    if by_number || by_id {
                        num(n.get("value"))
                    } else {
                        None
                    }
                })
            };
            let kcal100 = find("208", 1008)?;
            let unit_is_g = f
                .get("servingSizeUnit")
                .and_then(Value::as_str)
                .map(|u| u.eq_ignore_ascii_case("g") || u.eq_ignore_ascii_case("grm"))
                .unwrap_or(false);
            let serving = if unit_is_g {
                num(f.get("servingSize"))
                    .filter(|s| *s > 0.0)
                    .unwrap_or(100.0)
            } else {
                100.0
            };
            let k = serving / 100.0;
            let brand = f
                .get("brandName")
                .or_else(|| f.get("brandOwner"))
                .and_then(Value::as_str)
                .map(str::to_string);
            Some(ExternalFood {
                source: "usda".into(),
                external_id: id,
                name,
                brand,
                kcal: round1(kcal100 * k),
                protein_g: round1(find("203", 1003).unwrap_or(0.0) * k),
                carbs_g: round1(find("205", 1005).unwrap_or(0.0) * k),
                fat_g: round1(find("204", 1004).unwrap_or(0.0) * k),
                serving_g: serving,
            })
        })
        .take(MAX_PER_PROVIDER)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn off_scales_per_100g_to_serving() {
        let body = json!({"products": [
            {"code": "123", "product_name": "Granola", "brands": "Acme, Other",
             "serving_quantity": "40",
             "nutriments": {"energy-kcal_100g": 400, "proteins_100g": 10, "carbohydrates_100g": 60, "fat_100g": 15}},
            {"code": "999", "product_name": "No energy", "nutriments": {}}
        ]});
        let foods = parse_off(&body);
        assert_eq!(foods.len(), 1);
        assert_eq!(foods[0].kcal, 160.0);
        assert_eq!(foods[0].protein_g, 4.0);
        assert_eq!(foods[0].serving_g, 40.0);
        assert_eq!(foods[0].brand.as_deref(), Some("Acme"));
    }

    #[test]
    fn usda_defaults_to_100g() {
        let body = json!({"foods": [{"fdcId": 171688, "description": "Apples, raw",
            "foodNutrients": [
                {"nutrientNumber": "208", "value": 52},
                {"nutrientNumber": "203", "value": 0.26},
                {"nutrientNumber": "205", "value": 13.8},
                {"nutrientNumber": "204", "value": 0.17}]}]});
        let foods = parse_usda(&body);
        assert_eq!(foods[0].external_id, "171688");
        assert_eq!(foods[0].kcal, 52.0);
        assert_eq!(foods[0].serving_g, 100.0);
    }

    #[test]
    fn sanitize_rejects_garbage() {
        let mut f = ExternalFood {
            source: "off".into(),
            external_id: "1".into(),
            name: "  ".into(),
            brand: None,
            kcal: 10.0,
            protein_g: 0.0,
            carbs_g: 0.0,
            fat_g: 0.0,
            serving_g: 100.0,
        };
        assert!(f.clone().sanitized().is_none());
        f.name = "Ok".into();
        assert!(f.clone().sanitized().is_some());
        f.kcal = f64::NAN;
        assert!(f.sanitized().is_none());
    }
}
