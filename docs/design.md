# FitTrack design

Principles: flat, system fonts, lots of white space, no gradients, no heavy animation. Plain words over scores. Logging in under 10 seconds (one tap from a card). Max 4 tabs: **Home · Log · Friends · Settings**.

## Palette (original)
| Token | Light | Dark | Use |
|---|---|---|---|
| `bg` | `#FAFAF7` | `#121413` | screen background |
| `card` | `#FFFFFF` | `#1B1E1D` | cards (1px border `#E6E6E0`, no shadow) |
| `text` | `#1C1F1E` | `#ECEFEE` | primary text |
| `muted` | `#6B7270` | `#9AA3A0` | secondary text |
| `accent` | `#1F8A70` | `#3FBF9F` | bars, active tab (pine green) |
| `warn` | `#D98A1F` | `#E9A445` | "Overeating", "Tired" |

Font: system (SF Pro / Roboto). Sizes: 28 title, 20 card status, 15 body, 13 caption. Radius 12. Spacing 8-pt grid.

## Home mockup

```
┌──────────────────────────────────┐
│  Thursday                        │
│                                  │
│ ┌──────────────────────────────┐ │
│ │ Sleep                        │ │
│ │ Well rested          7 h 40m │ │
│ └──────────────────────────────┘ │
│                                  │
│ ┌──────────────────────────────┐ │
│ │ Diet                         │ │
│ │ On track                     │ │
│ │ ██████████░░░░░░  1,420/2,200│ │
│ └──────────────────────────────┘ │
│                                  │
│ ┌──────────────────────────────┐ │
│ │ Exercise                     │ │
│ │ 6,200 steps · Run 30 min     │ │
│ │ Strength ████░░░░░░          │ │
│ │ Cardio   ███████░░░          │ │
│ │ Focus    █████░░░░░          │ │
│ └──────────────────────────────┘ │
│                                  │
│ ┌──────────────────────────────┐ │
│ │ Weight                       │ │
│ │ 78.4 kg                    ↘ │ │
│ └──────────────────────────────┘ │
│                                  │
│  Home    Log    Friends  Settings│
└──────────────────────────────────┘
```

Strength/Cardio/Focus bars appear only when the v2 `stats` flag is on; otherwise the Exercise card shows activity only.

## Interaction
- Tapping a card opens its quick-log sheet: Sleep (bedtime, wake, 1–5 stars), Diet (search/favorites, one-tap add), Exercise (pick exercise, sets/reps/weight), Weight (number pad).
- Pull to refresh; offline shows a small "Saved on this device" caption instead of a banner.

## Status words
- Sleep: *Well rested* (readiness ≥ 70), *Okay* (40–69), *Tired* (< 40).
- Diet: *Under-eating* (< 70% of target by end of day), *On track*, *Overeating* (> 110%).
- Weight arrow: ↗ / → / ↘ from 7-day trend (±0.2 kg dead zone).

## Branding
Name "FitTrack", icon: a plain pine-green circle with a white rising line. No third-party game art, fonts or logos.
