//! Data checks for the built-in food catalog seeded by
//! `migrations/0016_food_catalog.sql`. The migration keeps one row per line so
//! this test can parse it without a database; CI fails if a row breaks the
//! energy-consistency rule.

use std::collections::{HashMap, HashSet};

const MIGRATION: &str = include_str!("../../migrations/0016_food_catalog.sql");

const CATEGORIES: [&str; 15] = [
    "Egyptian",
    "Grains & bread",
    "Meat & poultry",
    "Fish & seafood",
    "Eggs & dairy",
    "Legumes",
    "Vegetables",
    "Fruits",
    "Nuts & seeds",
    "Oils & fats",
    "Sweets & snacks",
    "Drinks",
    "Fast food",
    "Supplements",
    "World dishes",
];

#[derive(Debug)]
struct Row {
    external_id: String,
    name: String,
    name_ar: Option<String>,
    category: String,
    serving_g: f64,
    kcal: f64,
    protein_g: f64,
    carbs_g: f64,
    fat_g: f64,
}

/// Splits a SQL tuple body on top-level commas; `None` for `NULL`.
fn fields(body: &str) -> Vec<Option<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut was_quoted = false;
    let mut chars = body.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '\'' {
                if chars.peek() == Some(&'\'') {
                    chars.next();
                    cur.push('\'');
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
        } else if c == '\'' {
            quoted = true;
            was_quoted = true;
        } else if c == ',' {
            out.push(finish(&cur, was_quoted));
            cur.clear();
            was_quoted = false;
        } else if !c.is_whitespace() {
            cur.push(c);
        }
    }
    assert!(!quoted, "unterminated string in: {body}");
    out.push(finish(&cur, was_quoted));
    out
}

fn finish(raw: &str, was_quoted: bool) -> Option<String> {
    if !was_quoted && raw == "NULL" {
        None
    } else {
        Some(raw.to_string())
    }
}

fn rows() -> Vec<Row> {
    MIGRATION
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("('builtin:"))
        .map(|l| {
            let body = l
                .strip_prefix('(')
                .and_then(|l| l.strip_suffix("),").or_else(|| l.strip_suffix(')')))
                .unwrap_or_else(|| panic!("malformed row: {l}"));
            let f = fields(body);
            assert_eq!(f.len(), 10, "expected 10 fields: {l}");
            let num = |i: usize| -> f64 {
                f[i].as_deref()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or_else(|| panic!("field {i} is not a number: {l}"))
            };
            assert_eq!(f[9].as_deref(), Some("builtin"), "{l}");
            Row {
                external_id: f[0].clone().expect("external_id"),
                name: f[1].clone().expect("name"),
                name_ar: f[2].clone(),
                category: f[3].clone().expect("category"),
                serving_g: num(4),
                kcal: num(5),
                protein_g: num(6),
                carbs_g: num(7),
                fat_g: num(8),
            }
        })
        .collect()
}

#[test]
fn catalog_rows_are_energy_consistent() {
    let rows = rows();
    assert!(rows.len() >= 250, "only {} rows parsed", rows.len());
    for r in &rows {
        assert!(
            r.serving_g > 0.0,
            "{}: serving_g must be > 0",
            r.external_id
        );
        for v in [r.kcal, r.protein_g, r.carbs_g, r.fat_g] {
            assert!(
                v.is_finite() && v >= 0.0,
                "{}: negative value",
                r.external_id
            );
        }
        let computed = 4.0 * r.protein_g + 4.0 * r.carbs_g + 9.0 * r.fat_g;
        let tolerance = (0.10 * r.kcal).max(10.0);
        assert!(
            (r.kcal - computed).abs() <= tolerance,
            "{}: kcal {} vs 4P+4C+9F = {computed:.1}",
            r.external_id,
            r.kcal
        );
        // Macros can never weigh more than the serving.
        assert!(
            r.protein_g + r.carbs_g + r.fat_g <= r.serving_g,
            "{}: macros exceed serving weight",
            r.external_id
        );
    }
}

#[test]
fn catalog_ids_names_and_categories_are_valid() {
    let rows = rows();
    let mut ids = HashSet::new();
    let mut per_cat: HashMap<&str, usize> = HashMap::new();
    for r in &rows {
        assert!(r.external_id.len() > "builtin:".len(), "{}", r.external_id);
        assert!(
            ids.insert(r.external_id.as_str()),
            "duplicate {}",
            r.external_id
        );
        assert!(!r.name.trim().is_empty() && r.name.chars().count() <= 200);
        if let Some(ar) = &r.name_ar {
            assert!(!ar.trim().is_empty(), "{}: empty name_ar", r.external_id);
        }
        assert!(
            CATEGORIES.contains(&r.category.as_str()),
            "{}: unknown category {}",
            r.external_id,
            r.category
        );
        *per_cat.entry(r.category.as_str()).or_default() += 1;
    }
    for c in CATEGORIES {
        assert!(per_cat.get(c).copied().unwrap_or(0) > 0, "{c} is empty");
    }
    assert!(
        per_cat["Egyptian"] >= 70,
        "Egyptian has {}",
        per_cat["Egyptian"]
    );
}

#[test]
fn parser_handles_quotes_and_null() {
    let f = fields("'builtin:x', 'Ta''ameya, fried', NULL, 'A', 1.5");
    assert_eq!(
        f,
        vec![
            Some("builtin:x".into()),
            Some("Ta'ameya, fried".into()),
            None,
            Some("A".into()),
            Some("1.5".into()),
        ]
    );
}
