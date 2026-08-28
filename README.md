# UNRUN: FIXED POINT

完全な世界を毎フレーム複製せず、**内容アドレス方式の差分 snapshot** から時間を巻き戻す、1 ステージ構成の 2D パズルプラットフォーマーです。Braid の「時間そのものを操作する」発想に影響を受けつつ、ゲームルールと snapshot engine をゼロから実装しています。

外部画像・フォント・音声 asset は不要です。Rust と macroquad だけで Windows / macOS の両方で動作します。

## 起動

Rust 1.85 以降が必要です。

```sh
cargo run
```

初回だけ crates.io から依存 package を取得します。

## 操作

| 操作 | キー |
| --- | --- |
| 移動 | `A` / `D` または左右矢印 |
| ジャンプ | `Space` / `W` / 上矢印 |
| 時間を巻き戻す | `R` または左 `Shift` を押し続ける |
| ステージをリセット | `Backspace` |
| クリア後に再開 | `Enter` |

画面左上の `TIMELINE` が巻き戻せる時間です。最大 20 秒、60 fps 単位で記録されます。

## ステージのルール

中央手前の黄色い結晶は **FIXED POINT** です。一度触れると、その事実だけは時間を巻き戻しても消えません。

1. 最初の障害物をジャンプし、FIXED POINT に触れる
2. 紫色のゲートの前で `R` を押し続ける
3. プレイヤーが過去へ戻る一方、時間に逆行するゲートだけが上へ開く
4. ゲートが黄色に固定されたら、もう一度前進して出口へ向かう

前進だけでは閉じたゲートを通れません。「未来で結晶に触れ、その未来を消して過去へ情報だけを持ち帰る」のが解法です。

## 状態記録アーキテクチャ

ゲームは描画 frame と独立した 60 Hz の固定 timestep で進みます。各 simulation frame の終端で `GameState` を 34 byte の正規形式へ encode します。

```text
GameState(n - 1) ─┐
                  ├─ XOR ─ changed-byte bitmap ─ BLAKE3 ─ CAS
GameState(n) ─────┘                                  │
                                                    ▼
FrameDelta { content_id }
```

### 1. 差分 snapshot

前後の状態 byte を XOR し、変化した byte の位置を bitmap、値を連続 payload として保存します。巻き戻し時は同じ XOR を現在状態へ適用するだけで 1 frame 前へ戻れます。停止中など同じ差分が続く場合、後述の CAS により実体は一つしか保持されません。

状態長が変わる型にも対応できるよう、完全な before / after を持つ安全な fallback 形式もあります。

### 2. Content-addressed store

完全 checkpoint と差分 blob は、種別と内容から計算した 128-bit BLAKE3 ID で `HashMap` に格納します。

- 同一内容は同一 ID となり、自動的に重複排除
- 読み出し時に ID を再計算し、破損や種別違いを検出
- 120 frame ごとに完全 checkpoint を作成
- 20 秒を越えた履歴と分岐前の未来は mark-and-sweep で回収
- HUD の `CAS / DELTA SAVE` で blob 数、payload 量、完全 copy 比の削減率をリアルタイム表示

汎用実装は `src/timeline.rs` の `Timeline` と `SnapshotState` に分離され、ゲーム固有型へ依存しません。

### 3. 固定点レイヤー

世界は二層に分かれます。

| レイヤー | 内容 | 巻き戻し |
| --- | --- | --- |
| `GameState` | プレイヤー位置・速度・接地状態・ジャンプ猶予・animation・クリア状態 | 差分履歴から復元 |
| `FixedPointState` | 結晶に触れた事実・逆行ドアの開度・巻き戻し量 | 意図的に snapshot 対象外 |

ドアが「消した未来を覚えている」のは演出上の例外処理ではなく、snapshot 境界そのものをゲーム mechanic にしたものです。

## ソース構成

| ファイル | 役割 |
| --- | --- |
| `src/timeline.rs` | BLAKE3 CAS、XOR 差分、checkpoint、GC、frame rewind |
| `src/world.rs` | platformer 物理、collision、stage、snapshot codec、固定点 rule |
| `src/main.rs` | macroquad loop、入力、vector 描画、HUD、巻き戻し演出 |

## 検証

```sh
cargo test --all-targets
cargo clippy --all-targets -- -D warnings
```

テストには次を含みます。

- frame 単位の完全な巻き戻し
- 無変化 frame の CAS 重複排除
- 履歴上限と可変長 snapshot fallback
- GameState の byte 単位 round trip
- 前進だけではゲートを通れないこと
- 固定点を有効化し、巻き戻した場合だけ自動プレイヤーがステージをクリアできること

視覚 smoke test 用に `UNRUN_CAPTURE_PATH` を設定すると、起動後の frame を PNG へ書き出して自動終了します。

```sh
UNRUN_CAPTURE_PATH=/tmp/unrun.png cargo run
```
