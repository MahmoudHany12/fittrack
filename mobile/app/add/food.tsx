import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useRouter } from 'expo-router';
import { Minus, Plus, Star } from 'lucide-react-native';
import { useEffect, useMemo, useState } from 'react';
import {
  ActivityIndicator,
  Pressable,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  View,
} from 'react-native';

import {
  addFavorite,
  foodKeys,
  formatServing,
  isSearchable,
  listFoodLogs,
  listFavorites,
  newId,
  previewTotals,
  recentFoods,
  removeFavorite,
  searchFoods,
  stepServings,
  todayUtc,
  type Food,
} from '@/api/food';
import { saveFoodLog } from '@/sync/store';
import { LOCAL_KEY } from '@/sync/hooks';
import { RecentEntries } from '@/sync/RecentEntries';
import { Card } from '@/components/Card';
import { Body, Button, Caption, Title } from '@/components/ui';
import { fontSizes, radius, spacing, useTheme } from '@/theme';

function useDebounced<T>(value: T, ms: number): T {
  const [v, setV] = useState(value);
  useEffect(() => {
    const id = setTimeout(() => setV(value), ms);
    return () => clearTimeout(id);
  }, [value, ms]);
  return v;
}

function FoodRow({
  food,
  selected,
  favorite,
  onSelect,
  onToggleFavorite,
}: {
  food: Food;
  selected: boolean;
  favorite: boolean;
  onSelect: () => void;
  onToggleFavorite: () => void;
}) {
  const t = useTheme();
  return (
    <View
      style={[
        styles.row,
        { borderColor: selected ? t.accent : t.border, backgroundColor: t.card },
      ]}
    >
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={`Select ${food.name}`}
        onPress={onSelect}
        style={styles.rowMain}
      >
        <Text numberOfLines={1} style={{ fontSize: fontSizes.body, color: t.text }}>
          {food.name}
        </Text>
        <Caption>
          {Math.round(food.kcal)} kcal · {formatServing(food)}
          {food.brand ? ` · ${food.brand}` : ''}
        </Caption>
      </Pressable>
      <Pressable
        accessibilityRole="button"
        accessibilityLabel={favorite ? `Remove ${food.name} from favorites` : `Favorite ${food.name}`}
        hitSlop={8}
        onPress={onToggleFavorite}
        style={styles.star}
      >
        <Star
          size={20}
          color={favorite ? t.strength : t.muted}
          fill={favorite ? t.strength : 'transparent'}
        />
      </Pressable>
    </View>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <View style={{ gap: spacing(1) }}>
      <Caption>{title}</Caption>
      {children}
    </View>
  );
}

export default function AddFood() {
  const t = useTheme();
  const router = useRouter();
  const qc = useQueryClient();

  const [text, setText] = useState('');
  const query = useDebounced(text.trim(), 300);
  const searching = isSearchable(query);

  const [selected, setSelected] = useState<Food | null>(null);
  const [servings, setServings] = useState(1);
  // One id per add attempt: a retry after a flaky network cannot double-log.
  const [attemptId, setAttemptId] = useState(newId);

  const favorites = useQuery({ queryKey: foodKeys.favorites, queryFn: listFavorites });
  const recent = useQuery({ queryKey: foodKeys.recent, queryFn: recentFoods });
  const today = todayUtc();
  const todayLogs = useQuery({
    queryKey: ['food', 'logs', today],
    queryFn: () => listFoodLogs(today),
  });
  const results = useQuery({
    queryKey: foodKeys.search(query),
    queryFn: () => searchFoods(query),
    enabled: searching,
    staleTime: 60_000,
  });

  const favoriteIds = useMemo(
    () => new Set((favorites.data ?? []).map((f) => f.id)),
    [favorites.data],
  );

  const toggleFavorite = useMutation({
    mutationFn: (food: Food) =>
      favoriteIds.has(food.id) ? removeFavorite(food.id) : addFavorite(food.id),
    onSuccess: () => qc.invalidateQueries({ queryKey: foodKeys.favorites }),
  });

  const add = useMutation({
    mutationFn: () => {
      if (!selected) throw new Error('No food selected');
      return saveFoodLog({
        id: attemptId,
        food_id: selected.id,
        servings,
        ...previewTotals(selected, servings),
      });
    },
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: foodKeys.all });
      await qc.invalidateQueries({ queryKey: ['dashboard'] });
      await qc.invalidateQueries({ queryKey: LOCAL_KEY });
      if (router.canGoBack()) router.back();
      else router.replace('/');
    },
  });

  function select(food: Food) {
    setSelected(food);
    setServings(1);
    setAttemptId(newId());
    add.reset();
  }

  function renderFoods(foods: Food[]) {
    return foods.map((f) => (
      <FoodRow
        key={f.id}
        food={f}
        selected={selected?.id === f.id}
        favorite={favoriteIds.has(f.id)}
        onSelect={() => select(f)}
        onToggleFavorite={() => toggleFavorite.mutate(f)}
      />
    ));
  }

  const totals = selected ? previewTotals(selected, servings) : null;
  const favs = favorites.data ?? [];
  const recents = (recent.data ?? []).filter((f) => !favoriteIds.has(f.id));

  return (
    <View style={[styles.screen, { backgroundColor: t.bg }]}>
      <View style={{ gap: spacing(1.5) }}>
        <Title>Add food</Title>
        <TextInput
          accessibilityLabel="Search foods"
          placeholder="Search foods"
          placeholderTextColor={t.muted}
          value={text}
          onChangeText={setText}
          autoCapitalize="none"
          autoCorrect={false}
          returnKeyType="search"
          style={[
            styles.input,
            { backgroundColor: t.card, color: t.text, borderColor: t.border },
          ]}
        />
      </View>

      <ScrollView
        style={{ flex: 1 }}
        contentContainerStyle={{ gap: spacing(2), paddingBottom: spacing(2) }}
        keyboardShouldPersistTaps="handled"
      >
        {searching ? (
          <Section title="Results">
            {results.isFetching && !results.data ? (
              <ActivityIndicator color={t.accent} />
            ) : results.isError ? (
              <Caption warn>Search failed. Check your connection and try again.</Caption>
            ) : results.data && results.data.length > 0 ? (
              renderFoods(results.data)
            ) : (
              <Body muted>No foods found.</Body>
            )}
          </Section>
        ) : (
          <>
            {favs.length > 0 ? <Section title="Favorites">{renderFoods(favs)}</Section> : null}
            {recents.length > 0 ? <Section title="Recent">{renderFoods(recents)}</Section> : null}
            {favs.length === 0 && recents.length === 0 ? (
              <Body muted>Type at least 2 letters to search. Starred foods show up here.</Body>
            ) : null}
            <RecentEntries
              table="food_logs"
              server={todayLogs.data as unknown as { id: string; updated_at: string }[] | undefined}
              describe={(e) =>
                `${typeof e.data.food_name === 'string' ? e.data.food_name : 'Food'} · ${Math.round(
                  Number(e.data.kcal) || 0,
                )} kcal`
              }
            />
          </>
        )}
      </ScrollView>

      {selected && totals ? (
        <Card>
          <View style={{ gap: spacing(1.5) }}>
            <View>
              <Text numberOfLines={1} style={{ fontSize: fontSizes.cardTitle, color: t.text }}>
                {selected.name}
              </Text>
              <Caption>
                {totals.kcal} kcal · P {totals.protein_g} · C {totals.carbs_g} · F {totals.fat_g}
              </Caption>
            </View>
            <View style={styles.stepper}>
              <Pressable
                accessibilityRole="button"
                accessibilityLabel="Fewer servings"
                onPress={() => setServings((s) => stepServings(s, -1))}
                style={[styles.stepBtn, { borderColor: t.border, backgroundColor: t.bg }]}
              >
                <Minus size={20} color={t.text} />
              </Pressable>
              <Text
                accessibilityLabel={`${servings} servings`}
                style={{ fontSize: fontSizes.cardTitle, color: t.text, minWidth: 96, textAlign: 'center' }}
              >
                {servings} {servings === 1 ? 'serving' : 'servings'}
              </Text>
              <Pressable
                accessibilityRole="button"
                accessibilityLabel="More servings"
                onPress={() => setServings((s) => stepServings(s, 1))}
                style={[styles.stepBtn, { borderColor: t.border, backgroundColor: t.bg }]}
              >
                <Plus size={20} color={t.text} />
              </Pressable>
            </View>
            {add.isError ? <Caption warn>Could not add. Try again.</Caption> : null}
            <Button title="Add" busy={add.isPending} onPress={() => add.mutate()} />
          </View>
        </Card>
      ) : null}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: { flex: 1, padding: spacing(3), paddingTop: spacing(6), gap: spacing(2) },
  input: {
    borderWidth: 1,
    borderRadius: radius,
    paddingHorizontal: spacing(1.5),
    paddingVertical: spacing(1.5),
    fontSize: fontSizes.body,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    borderWidth: 1,
    borderRadius: radius,
  },
  rowMain: { flex: 1, padding: spacing(1.5), gap: spacing(0.5) },
  star: { padding: spacing(1.5) },
  stepper: { flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: spacing(2) },
  stepBtn: {
    width: 48,
    height: 48,
    borderRadius: radius,
    borderWidth: 1,
    alignItems: 'center',
    justifyContent: 'center',
  },
});
