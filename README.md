# UNRUN: FIXED POINT

完全な世界を毎フレーム複製せず、**内容アドレス方式の差分 snapshot** から時間を巻き戻す、4 ステージ構成（3 + ボーナス）の 2D パズルプラットフォーマーです。Braid の「時間そのものを操作する」発想に影響を受けつつ、ゲームルールと snapshot engine をゼロから実装しています。BGM と効果音は外部素材を使わず、Rust で生成した UK garage（132 BPM の 2-step / shuffle / sub-bass）ループと合成音です。ボーナスステージでは同じ世界観のまま Rust の所有権・match・イテレータを題材にしたコードパズルを解きます。

外部画像・フォント・音声 asset は不要です。native Rust、macroquad、BLAKE3 を使用し、Linux / macOS / Windows で CI 検証しています。ブラウザ / WASM は対象外です。

描画は固定解像度の中間 texture を使わず、OS が提供する native framebuffer へ直接出力します。Retina などの高 DPI 環境では物理解像度に合わせて glyph を再 rasterize するため、ウィンドウを拡大しても文字がぼけません。

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
| クリア後に次へ | `Enter` |
| BGM / SE ミュート | `M` |
| ボーナス端末をハック | `E`（端末の近くで） |
| 回答コードをチェック | `Ctrl+Enter` または `F5`（エディタ内） |
| エディタを閉じる | `Esc` |

BGM は UK garage のループを stereo WAV として runtime 生成し、`macroquad::audio` でループ再生します。ジャンプ・FIXED POINT 接触・ゲート固定・ステージクリアは合成効果音、巻き戻し中は専用のドローンが重なります。巻き戻し中は BGM を自動で duck します。`M` でミュートを切り替えられます。

画面左上の `TIMELINE` が巻き戻せる時間です。最大 20 秒、60 fps 単位で記録されます。

## ステージのルール

中央手前の黄色い結晶は **FIXED POINT** です。一度触れると、その事実だけは時間を巻き戻しても消えません。紫色のゲートは時間を巻き戻したときだけ開きます。

### Stage 1 — FIRST CONTACT

チュートリアルです。最初の障害物を越えて FIXED POINT に触れ、ゲート前で `R` を長押しして開け、出口へ向かいます。

### Stage 2 — THE DROP

舞台は上下二層です。真ん中の 150px 幅の穴へ落ちると、地上へは戻れません。下層の奥にある FIXED POINT まで `→` で走り抜け、ゲートまで戻って `R` を長押しします。プレイヤーだけが再び上層へ戻り、二度目はゲートを越えて上層の出口へ進みます。

### Stage 3 — B-SIDE

スタート地点が左右の中央にあり、FIXED POINT は右奥、出口は左奥です。まず右へ進んで FIXED POINT を取り、今度は左へ引き返しながらゲート前で `R` を長押しします。進行方向そのものが反転するパズルです。

### Bonus — RUST FORGE

Stage 3 をクリアして `Enter` で進む第 4 ステージ（ボーナス）です。永続的な unlock や save はありません。世界観はそのままに、タイムラインのソースが壊れています。ステージ内の 3 つの端末（`01 // OWNERSHIP` / `02 // MATCH` / `03 // ITERATOR`）に近づいて `E` でエディタを開き、Rust 風の回答コードを編集します。

- **OWNERSHIP**: `let past = timeline;` でムーブしてしまったタイムラインを `clone()` や `&` で借用できるように修正
- **MATCH**: `fixed && rewound >= 75` のときだけ `Gate::Open` を返す分岐を実装
- **ITERATOR**: `iter().filter(|n| *n % 2 == 0).sum()` で偶数だけを集計するイテレータチェーンを完成

`Ctrl+Enter` または `F5` で回答をチェックし、受理されると端末が点灯します。判定はコメントと文字列を除外した token 列を想定解と照合する簡易 validator であり、`rustc` によるコンパイルやコード実行は行いません。3 つとも受理されると `ANOMALY ACCEPTED` と表示され FIXED POINT が自動で点灯し、あとは通常通り `R` でゲートを開けて出口へ向かいます。Rust の所有権・パターンマッチ・イテレータが、そのままゲーム内の時間・ゲート・差分圧縮のメタファーになっています。

前進だけではどの面の閉じたゲートも通れません。通常の 3 ステージでは「未来で結晶に触れ、その未来を消して過去へ情報だけを持ち帰る」のが共通の解法です。ボーナスでは 3 つの端末を解くと FIXED POINT が点灯します。

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

差分 blob は、内容から計算した 128-bit BLAKE3 ID で `HashMap` に格納します。

- 同一内容は同一 ID となり、自動的に重複排除
- 読み出し時に ID を再計算し、破損を検出
- 履歴は現在から 1 frame ずつ後方へ適用する delta のみを保持
- 20 秒を越えた履歴と分岐前の未来は、破棄が 120 frame 分たまった時点で mark-and-sweep により回収
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
| `src/audio.rs` | macroquad への音声ロード・再生・ミュート制御 |
| `src/editor.rs` | ボーナスエディタの状態、入力反映、validator 呼び出し |
| `src/game.rs` | `Game`、ステージ遷移、巻き戻し orchestration |
| `src/render.rs` | camera、vector 描画、HUD、エディタ表示、巻き戻し演出 |
| `src/sound.rs` | UK garage ループと効果音の stereo WAV 生成（外部 asset なし） |
| `src/timeline.rs` | BLAKE3 CAS、XOR 差分、GC、frame rewind |
| `src/world.rs` | platformer 物理、collision、4 ステージ定義、ボーナス端末・Rust パズル、snapshot codec、固定点 rule |
| `src/main.rs` | window 設定、macroquad loop、入力採取、各 module の配線 |

## 検証

format、全 unit test、Clippy、実 framebuffer の orientation test をまとめて実行できます。最後の test では数 frame だけ検証用ウィンドウが開きます。

```sh
./scripts/check.sh
```

個別に実行する場合は次のコマンドを使います。

```sh
cargo test --locked --all-targets
cargo clippy --locked --all-targets -- -D warnings
```

テストには次を含みます。

- frame 単位の完全な巻き戻し
- 無変化 frame の CAS 重複排除
- 履歴上限と可変長 snapshot fallback
- GameState の byte 単位 round trip
- 前進だけでは Stage 1 のゲートを通れないこと
- 全 4 ステージの scripted rewind 解法が成立すること
- ボーナス端末の validator が想定解を受理し、壊れた回答、未編集 stub、コメント・文字列だけの偽装を拒否すること
- 生成した UK garage ループが有効な stereo WAV であること

視覚 smoke test 用に `UNRUN_CAPTURE_PATH` を設定すると、起動後の frame を PNG へ書き出して自動終了します。

```sh
UNRUN_CAPTURE_PATH=/tmp/unrun.png cargo run
```

画面の上下・左右反転を実 framebuffer の四隅で検査する integration test も実行できます。

```sh
cargo run --locked -- --visual-test
```

orientation test は左上=赤、右上=緑、左下=青、右下=黄の probe を GPU で描き、screen readback の pixel を直接照合します。validator 自体にも正常・上下反転・左右反転の unit test があります。GitHub Actions では Linux / macOS / Windows の各 runner で format・unit test・Clippy を実行し、Linux の仮想 display 上で GPU test も実行します。

## ライセンス

[MIT License](LICENSE)
