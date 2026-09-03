# unrun 未対応指摘リスト

このファイルは追記専用です。既存の指摘は削除・上書き・並べ替えせず、対応状況も末尾へ追記します。

## #1 正解受理後に同じ成功イベントを何度でも再発火できる

- 検出日: 2026-08-29
- 対象コミット: `85376b8682b6e60b31c473628327ccd40cdeedcc`
- 対象箇所:
  - `src/editor.rs:60-68` `EditorState::update`
  - `src/editor.rs:94-105` `EditorState::submit`
  - `src/game.rs:165-188` `Game::update`
- 指摘内容:
  正解を受理してもエディタの `open` が `true` のままです。そのため、同じ画面で `Ctrl+Enter` または `F5` を再度押すたびに `EditorEvent::Submitted { accepted: true }` が返ります。`Game::update` はそのたびに `GameEvents::fixed_point_activated = true` を設定し、3問完了後は `GameEvents::gate_latched = true` も繰り返し設定します。
- なぜ問題か:
  1回だけ起きるべき端末解決イベントが入力回数分発火し、成功通知や効果音を繰り返せます。`bonus_solved[terminal]` を `true` にする処理は冪等でも、イベントは冪等ではありません。今後イベントに実績、保存、演出などを追加すると、重複処理の原因になります。
- 推奨する直し方:
  正解受理時にエディタを閉じるのが単純です。成功表示をエディタ内に残す必要がある場合は、`EditorState` に `accepted: bool` を持たせ、受理後の submit を無視してください。失敗後は従来どおり編集と再 submit ができるようにします。

  次のテストを追加してください。
  - `src/editor.rs`: 正解受理後に submit を再入力しても、2回目の `Submitted { accepted: true }` が発生しないこと。
  - `src/game.rs`: 同じ端末の成功処理で `fixed_point_activated` と `gate_latched` が一度しか発火しないこと。

## #2 `cargo run` が無言で終了する現象の再現情報を採取する

- 検出日: 2026-08-29
- 対象コミット: `85376b8682b6e60b31c473628327ccd40cdeedcc`
- 対象箇所:
  - 実行環境
  - `src/main.rs:23-100` `main`
- 種別: 調査項目。現 HEAD では未再現。
- 指摘内容:
  ユーザー環境で `cargo run` が無言で終了する現象が報告されています。ただし、現 HEAD の通常実行パスには自動終了処理はありません。明示的に終了するのは `--visual-test` を渡した場合、または `UNRUN_CAPTURE_PATH` を設定した場合の3フレーム後だけです。現時点ではコード上の原因を特定できず、環境変数、引数、起動方法、OS などに依存する可能性があります。
- なぜ問題か:
  再現条件と診断情報がない状態では、正常な終了、panic、OS による終了、描画・音声 backend の初期化失敗、意図しないテスト用設定を区別できません。推測でコードを変えると、本来正常な経路を壊す可能性があります。
- 推奨する調査方法:
  この項目はコーディングAIだけでは修正できません。まずユーザー側で、現象が起きた同じターミナルから次の情報を採取してください。

  1. OS の名前とバージョン、利用しているターミナルの名前。
  2. 実際に実行したコマンド全文と、渡した全引数。
  3. `UNRUN_` で始まる環境変数を含む、実行時の関連環境変数。特に `UNRUN_CAPTURE_PATH`、`UNRUN_EDITOR_CAPTURE`、`UNRUN_START_STAGE` の有無と値。
  4. 終了直後の exit code。
     - macOS / Linux: `echo $?`
     - PowerShell: `$LASTEXITCODE`
  5. `RUST_BACKTRACE=1` を設定した再実行時の stdout / stderr 全文と backtrace。
     - macOS / Linux: `RUST_BACKTRACE=1 cargo run 2>&1 | tee unrun-run.log`
     - PowerShell: `$env:RUST_BACKTRACE='1'; cargo run *>&1 | Tee-Object unrun-run.log`

  情報がそろってから、テスト用の引数・環境変数による意図した終了なのか、panic / backend 障害なのかを切り分けてください。
