# wezterm CLI フィールド検証結果

## 検証環境

- **wezterm バージョン**: `20240203-110809-5046fc22`
- **OS**: Darwin (macOS)
- **検証日**: 2026-01-19

## 利用可能フィールド

`wezterm cli list --format json` で取得できるフィールド:

| フィールド名 | 型 | 例 | wzcc での用途 | 重要度 |
|-------------|----|----|--------------|--------|
| `pane_id` | number | `0` | ペイン識別 | **必須** |
| `tty_name` | string | `/dev/ttys003` | tty マッチング (Case 2) | **高** |
| `title` | string | `nvim` | title パターン検出 (fallback) | **高** |
| `cwd` | string | `file:///Users/furukawa/dotfiles` | CWD マッチング (補助) | 中 |
| `is_active` | boolean | `false` | フォーカス判定 | **高** |
| `tab_id` | number | `0` | Tab アクティベーション | **高** |
| `window_id` | number | `0` | Window 識別 | 中 |
| `workspace` | string | `default` | Workspace 識別 | 低 |
| `tab_title` | string | `dotfiles` | UI 表示用 | 低 |
| `window_title` | string | `✽ Implementation request` | UI 表示用 | 低 |
| `cursor_x` | number | `58` | (未使用) | 低 |
| `cursor_y` | number | `15` | (未使用) | 低 |
| `cursor_shape` | string | `SteadyBlock` | (未使用) | 低 |
| `cursor_visibility` | string | `Visible` | (未使用) | 低 |
| `left_col` | number | `0` | (未使用) | 低 |
| `top_row` | number | `0` | (未使用) | 低 |
| `size` | object | `{rows: 63, cols: 210, ...}` | (未使用) | 低 |
| `is_zoomed` | boolean | `false` | (未使用) | 低 |

## 取得できないフィールド

- `foreground_process_name`: wezterm CLI では提供されていない
  - → Case 1 (プロセス名での直接マッピング) は **使用不可**
  - → Case 2 (tty マッチング) を第一候補とする

## 検出戦略の分岐 (Phase 0 の結論)

| Case | 条件 | 検出方法 | 精度 | 実装Priority | 判定 |
|------|------|----------|------|--------------|------|
| ~~Case 1~~ | ~~`foreground_process_name` が取得可能~~ | ~~プロセス名でフィルタ → 即確定~~ | ~~高 (95%+)~~ | ~~Phase 2.1~~ | **❌ 使用不可** |
| **Case 2** | **`tty_name` が取得可能** | **tty で ps と突合 → 確定** | **高 (90%+)** | **Phase 2.1** | **✅ 第一候補** |
| Case 3 | `tty_name` が失敗 | プロセスツリー + CWD + title でスコアリング | 中 (70-80%) | Phase 2.2 | ✅ fallback |
| Case 4 | 上記すべて失敗 | title パターンのみ (低精度) + 要確認に落とす | 低 (50-60%) | Phase 2.2 | ✅ 最終 fallback |

## wzcc 実装への影響

### ✅ 使用可能な検出方法

1. **第一候補: tty マッチング (Case 2)**
   - `tty_name` と `ps aux` の TTY カラムを突合
   - 精度: 高 (90%+)
   - macOS では `/dev/ttys003` 形式

2. **第二候補: ヒューリスティック推定 (Case 3)**
   - プロセスツリー + CWD + title のスコアリング
   - 精度: 中 (70-80%)

3. **最終 fallback: title パターン (Case 4)**
   - title に `✳` やスピナーが含まれるか
   - 精度: 低 (50-60%)
   - 要確認 UI に落とす

### ✅ フォーカス判定方法

- `is_active` フィールドを使用
- wzcc 自身がフォーカス中かを判定可能
- 動的ポーリング間隔の切り替えに使用

### ⚠️ 注意事項

- `foreground_process_name` が使えないため、Case 1 は実装しない
- `tty_name` が第一候補となる
- スコアリング方式は fallback として実装

## 対応範囲の宣言

- **保証**: WezTerm v20240203-110809+ / macOS 14+
- **ベストエフォート**: Linux (同様のフィールドが提供される想定)
- **非対応**: remote mux (Phase 5 以降で検討)
- **非対応**: Windows (TTY の概念が異なる)

## 次のステップ

1. Phase 2.1 で tty マッチングを実装 (Case 2)
2. Phase 2.2 でスコアリング fallback を実装 (Case 3, 4)
3. `src/detector/identify.rs` で Case 2〜4 の分岐ロジックを実装
