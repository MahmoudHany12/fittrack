# FitTrack design

Direction (v2 of the design, after owner feedback): **dark-first, calm, card-based**, inspired by the *feel* of a phone-OS health widget — a name + one status word at the top, small section headings ("Fitness", "Body & Nutrition"), and one dark card per metric with a **circular progress ring + icon** and a plain "Level n/4" label. We match the simplicity, not any game's artwork: our own colors, icons, fonts (system), copy and layout details. No gradients, no heavy animation, lots of empty space. Max 4 tabs: **Home · Log · Friends · Settings**. Logging in under 10 seconds (one tap on a card).

## Palette (original; dark default, light supported)
| Token | Dark | Light | Use |
|---|---|---|---|
| `bg` | `#0E1110` | `#F6F7F4` | screen |
| `card` | `#171B1A` | `#FFFFFF` | cards (1px border `#242A28` / `#E3E5E0`, radius 16, no shadow) |
| `text` | `#EEF1EF` | `#1B1F1E` | primary |
| `muted` | `#8C9794` | `#6A7371` | secondary |
| `strength` | `#F2A65A` | `#D9822B` | amber ring |
| `cardio` | `#4FD1B5` | `#1F8A70` | pine/teal ring |
| `focus` | `#8FA8FF` | `#4C63D9` | periwinkle ring |
| `good` / `warn` | `#5FD38D` / `#F2B24A` | darker equivalents | status words |

Font: system (SF Pro / Roboto). Sizes: 26 title, 18 card title, 14 body, 12 caption. 8-pt grid. Icons: Feather/Lucide-style line icons (MIT/ISC, `lucide-react-native`).

## Home mockup (dark)

```
┌──────────────────────────────────┐
│  Alex                        (A) │
│  Well rested                     │   <- status word from readiness
│                                  │
│  Fitness                         │
│ ┌──────────────────────────────┐ │
│ │ Strength            ╭───╮    │ │
│ │ Level 1/4           │ ♡ │ ◜  │ │   ring = progress to next level
│ │                     ╰───╯    │ │
│ └──────────────────────────────┘ │
│ ┌──────────────────────────────┐ │
│ │ Cardio              ╭───╮    │ │
│ │ Level 2/4           │ ϟ │ ◕  │ │
│ │ ▇▇▇▇▇░░░░░░  Today's window  │ │   <- bar: today's activity vs goal
│ └──────────────────────────────┘ │
│ ┌──────────────────────────────┐ │
│ │ Focus               ╭───╮    │ │
│ │ Level 3/4           │ ◉ │ ◔  │ │
│ └──────────────────────────────┘ │
│                                  │
│  Body & Nutrition                │
│ ┌──────────────────────────────┐ │
│ │ Sleep   7 h 40m   Well rested│ │
│ │ Diet    1,420 / 2,200  ▇▇▇░░ │ │
│ │ Weight  78.4 kg            ↘ │ │
│ └──────────────────────────────┘ │
│                                  │
│  Home    Log    Friends  Settings│
└──────────────────────────────────┘
```

- Strength / Cardio / Focus rings and levels need the v2 `stats` flag. With it off, the Fitness section shows a single Activity card (steps + today's workout).
- Level n/4: each stat is 0–100 mapped to 4 levels (0–24 → 1 … 75–100 → 4); the ring shows progress inside the current level.
- Sleep, Diet and Weight are compact rows inside one "Body & Nutrition" card; tapping a row opens its quick-log sheet.

## Interaction
- Tap a card → quick-log sheet (Strength/Cardio: pick exercise, sets/reps/weight or duration; Focus: logs from sleep + activity automatically, tap shows why; Sleep: bedtime, wake, 1–5 stars; Diet: search/favorites/one-tap add; Weight: number pad).
- Pull to refresh. Offline shows a small "Saved on this device" caption instead of a banner.

## Status words
- Header (readiness): *Well rested* ≥ 70, *Okay* 40–69, *Tired* < 40, *No data yet*.
- Diet: *Under-eating* (< 70% of target by evening), *On track*, *Overeating* (> 110%).
- Weight arrow: ↗ → ↘ from the 7-day trend (±0.2 kg dead zone).

## Branding
Name "FitTrack"; icon: pine-green circle with a white rising line. No third-party game names, art, fonts or logos.
