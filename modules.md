# financeATP 実装モジュール一覧

このドキュメントは、financeATPの実装を小さなモジュールに分解したタスクリストです。
各モジュールは独立して実装・テスト可能な単位になっています。

---

## 凡例

- ⬜ 未着手
- 🔄 進行中
- ✅ 完了
- 🔴 ブロック中

---

## Phase 1: データベース基盤

### 1.1 PostgreSQL拡張機能
- ✅ **M001** `uuid-ossp` 拡張の有効化
- ✅ **M002** `pgcrypto` 拡張の有効化

### 1.2 共通関数・トリガー
- ✅ **M003** `prevent_event_modification()` トリガー関数の実装
- ✅ **M004** イミュータブルテーブル用トリガーの作成

---

## Phase 2: 認証・認可テーブル

### 2.1 APIキー管理
- ✅ **M005** `api_keys` テーブルの作成
- ✅ **M006** `api_keys` インデックスの作成
- ✅ **M007** 初期APIキーのシード投入

### 2.2 Rate Limiting
- ✅ **M008** `rate_limit_buckets` テーブルの作成
- ✅ **M009** `check_and_increment_rate_limit()` 関数の実装
- ✅ **M010** Rate Limitバケットクリーンアップ関数の実装

---

## Phase 3: イベントソーシング基盤

### 3.1 イベントストア
- ✅ **M011** `events` テーブルの作成（パーティション対応）
- ✅ **M012** 2026年1月パーティションの作成
- ✅ **M013** 2026年2月パーティションの作成
- ✅ **M014** `events` インデックスの作成
- ✅ **M015** `events` イミュータブルトリガーの適用
- ✅ **M016** `unique_idempotency` 制約の作成

### 3.2 スナップショット
- ✅ **M017** `event_snapshots` テーブルの作成
- ✅ **M018** `event_snapshots` インデックスの作成

---

## Phase 4: ユーザー関連テーブル

### 4.1 ユーザー
- ✅ **M019** `users` テーブルの作成
- ✅ **M020** `users` 入力検証制約の作成
- ✅ **M021** `users` インデックスの作成
- ✅ **M022** システムユーザー（SYSTEM_MINT, SYSTEM_FEE, SYSTEM_RESERVE）のシード投入

---

## Phase 5: 口座関連テーブル

### 5.1 勘定科目マスタ
- ✅ **M023** `account_types` テーブルの作成
- ✅ **M024** 勘定科目マスタデータのシード投入

### 5.2 口座
- ✅ **M025** `accounts` テーブルの作成
- ✅ **M026** `user_wallet_only` 制約の作成
- ✅ **M027** `get_wallet_account_id()` 関数の実装
- ✅ **M028** `accounts` インデックスの作成
- ✅ **M029** システムユーザー口座のシード投入

### 5.3 残高
- ✅ **M030** `account_balances` テーブルの作成
- ✅ **M031** 残高制約（非負、最大額）の作成
- ✅ **M032** `user_balances` ビューの作成
- ✅ **M033** システムユーザー残高レコードのシード投入

---

## Phase 6: 複式簿記テーブル

### 6.1 仕訳帳
- ✅ **M034** `ledger_entries` テーブルの作成（パーティション対応）
- ✅ **M035** 2026年1月パーティションの作成
- ✅ **M036** 2026年2月パーティションの作成
- ✅ **M037** `ledger_entries` インデックスの作成

### 6.2 複式簿記バランスチェック
- ✅ **M038** `check_ledger_balance_batch()` 関数の実装
- ✅ **M039** `validate_ledger_balance` トリガーの作成（STATEMENT レベル）

---

## Phase 7: 冪等性・監査ログテーブル

### 7.1 冪等性キー
- ✅ **M040** `idempotency_keys` テーブルの作成
- ✅ **M041** `idempotency_keys` インデックスの作成
- ✅ **M042** `reset_stale_idempotency_keys()` 関数の実装

### 7.2 監査ログ
- ✅ **M043** `audit_logs` テーブルの作成
- ✅ **M044** `audit_logs` インデックスの作成
- ✅ **M045** `calculate_audit_hash()` 関数の実装（排他ロック付き）
- ✅ **M046** `hash_audit_log` トリガーの作成
- ✅ **M047** `audit_logs` イミュータブルトリガーの適用

---

## Phase 8: Rust 基盤

### 8.1 プロジェクト設定
- ✅ **M048** Cargo.toml 依存関係の設定
- ✅ **M049** 環境変数設定（.env）
- ✅ **M050** ロギング設定（tracing）

### 8.2 データベース接続
- ✅ **M051** SQLx 接続プール設定
- ✅ **M052** マイグレーション実行機能

---

## Phase 9: ドメインモデル

### 9.1 Amount型
- ✅ **M053** `Amount` 構造体の実装
- ✅ **M054** `AmountError` の実装
- ✅ **M055** `Amount::new()` ビジネスルール検証の実装
- ✅ **M056** `Amount` のユニットテスト

### 9.2 操作コンテキスト
- ✅ **M057** `OperationContext` 構造体の実装

### 9.3 イベント定義
- ✅ **M058** `AccountEvent` enum の実装
- ✅ **M059** `TransferEvent` enum の実装
- ✅ **M060** `UserEvent` enum の実装
- ✅ **M061** `TransferFailureReason` enum の実装

---

## Phase 10: Aggregate 実装

### 10.1 Account Aggregate
- ✅ **M062** `Account` 構造体の実装
- ✅ **M063** `Account::create()` の実装
- ✅ **M064** `Account::apply()` の実装
- ✅ **M065** `Account::debit()` の実装
- ✅ **M066** `Account::credit()` の実装
- ✅ **M067** `Account::should_snapshot()` の実装
- ✅ **M068** `Account` のユニットテスト

### 10.2 User Aggregate
- ✅ **M069** `User` 構造体の実装
- ✅ **M070** `User::create()` の実装
- ✅ **M071** `User::apply()` の実装
- ✅ **M072** `User::update()` の実装
- ✅ **M073** `User` のユニットテスト

---

## Phase 11: イベントストア実装

### 11.1 基本機能
- ✅ **M074** `EventStore` 構造体の実装
- ✅ **M075** `AggregateOperation` 構造体の実装
- ✅ **M076** `EventStoreError` の実装

### 11.2 イベント追加
- ✅ **M077** `EventStore::try_append_atomic()` の実装
- ✅ **M078** `EventStore::append_atomic()` の実装（リトライ付き）
- ✅ **M079** 楽観的ロック検証の実装
- ✅ **M080** イベント追加のインテグレーションテスト

### 11.3 Aggregate ロード
- ✅ **M081** `EventStore::load_aggregate()` の実装
- ✅ **M082** スナップショットからのロード実装
- ✅ **M083** Aggregate ロードのインテグレーションテスト

### 11.4 スナップショット
- ✅ **M084** `EventStore::save_snapshot_if_needed()` の実装
- ✅ **M085** スナップショット作成のインテグレーションテスト

---

## Phase 12: Projection サービス

### 12.1 残高更新
- ✅ **M086** `ProjectionService` 構造体の実装
- ✅ **M087** `ProjectionService::apply_transfer()` の実装
- ✅ **M088** `account_balances` 更新ロジックの実装
- ✅ **M089** `ledger_entries` 挿入ロジックの実装
- ✅ **M090** Projection 更新のインテグレーションテスト

---

## Phase 13: 冪等性サービス

### 13.1 冪等性リポジトリ
- ✅ **M091** `IdempotencyRepository` 構造体の実装
- ✅ **M092** `IdempotencyRepository::get()` の実装
- ✅ **M093** `IdempotencyRepository::start_processing()` の実装
- ✅ **M094** `IdempotencyRepository::mark_completed()` の実装
- ✅ **M095** `IdempotencyRepository::mark_failed()` の実装
- ✅ **M096** 冪等性のインテグレーションテスト

---

## Phase 14: コマンドハンドラ

### 14.1 ユーザー作成
- ✅ **M097** `CreateUserCommand` の実装
- ✅ **M098** `CreateUserCommandHandler` の実装
- ✅ **M099** ユーザー作成時の口座自動作成ロジック
- ✅ **M100** ユーザー作成のインテグレーションテスト

### 14.2 送金
- ✅ **M101** `TransferCommand` の実装
- ✅ **M102** `TransferCommandHandler` の実装
- ✅ **M103** X-Request-User-Id 認可チェックの実装
- ✅ **M104** user_id → account_id 変換の実装
- ✅ **M105** 送金のインテグレーションテスト
- ✅ **M106** 残高不足エラーのテスト
- ✅ **M107** 同時送金（楽観的ロック）のテスト

### 14.3 ATP発行（Mint）
- ✅ **M108** `MintCommand` の実装
- ✅ **M109** `MintCommandHandler` の実装
- ✅ **M110** SYSTEM_MINT からの発行ロジック
- ✅ **M111** ATP発行のインテグレーションテスト

---

## Phase 15: APIキー認証

### 15.1 認証ミドルウェア
- ✅ **M112** `ApiKeyAuthMiddleware` の実装
- ✅ **M113** APIキー検証ロジックの実装
- ✅ **M114** 権限チェックロジックの実装
- ✅ **M115** 認証失敗時のエラーレスポンス

### 15.2 Rate Limiting ミドルウェア
- ✅ **M116** `RateLimitMiddleware` の実装
- ✅ **M117** Rate Limit 超過時のエラーレスポンス

### 15.3 ロギング
- ✅ **M118** `mask_headers_for_logging()` の実装
- ✅ **M119** リクエストログ出力ミドルウェア

---

## Phase 16: API エンドポイント

### 16.1 ユーザー API
- ✅ **M120** `POST /users` ハンドラの実装
- ✅ **M121** `GET /users/:user_id` ハンドラの実装
- ✅ **M122** `PATCH /users/:user_id` ハンドラの実装
- ✅ **M123** `DELETE /users/:user_id` ハンドラの実装

### 16.2 残高・履歴 API
- ✅ **M124** `GET /users/:user_id/balance` ハンドラの実装
- ✅ **M125** `GET /users/:user_id/history` ハンドラの実装

### 16.3 送金 API
- ✅ **M126** `POST /transfers` ハンドラの実装
- ✅ **M127** `GET /transfers/:transfer_id` ハンドラの実装

### 16.4 管理 API
- ✅ **M128** `POST /admin/mint` ハンドラの実装
- ⚠️ **M129** `POST /admin/burn` ハンドラの実装（スケルトンのみ）
- ✅ **M130** `GET /admin/events` ハンドラの実装

---

## Phase 17: エラーハンドリング

### 17.1 エラー型
- ✅ **M131** `AppError` enum の実装
- ✅ **M132** `DomainError` の実装
- ✅ **M133** エラーからHTTPレスポンスへの変換
- ✅ **M134** エラーレスポンスJSON形式の実装

---

## Phase 18: Axum ルーター

### 18.1 ルーター設定
- ⚠️ **M135** `AppState` 構造体の実装（PgPoolを直接使用で代替）
- ✅ **M136** ルーター設定（Router::new()）
- ✅ **M137** ミドルウェアスタック設定
- ✅ **M138** CORSなし（内部API）確認

### 18.2 サーバー起動
- ✅ **M139** サーバー起動コード（main.rs）
- ✅ **M140** グレースフルシャットダウンの実装

---

## Phase 19: 監査ログ

### 19.1 監査ログサービス
- ⬜ **M141** `AuditLogService` の実装
- ⬜ **M142** 監査ログ書き込みロジック
- ⬜ **M143** 監査ログ検証ロジック（ハッシュチェーン）

---

## Phase 20: 定期ジョブ

### 20.1 クリーンアップジョブ
- ⬜ **M144** Rate Limit バケットクリーンアップジョブ
- ⬜ **M145** 冪等性キータイムアウトリセットジョブ
- ⬜ **M146** 期限切れ冪等性キー削除ジョブ

### 20.2 パーティション管理
- ⬜ **M147** 月次パーティション自動作成スクリプト

---

## Phase 21: バックアップ

### 21.1 バックアップ設定
- ⬜ **M148** postgresql.conf WALアーカイブ設定
- ⬜ **M149** 日次フルバックアップスクリプト
- ⬜ **M150** バックアップ保持ポリシースクリプト

---

## Phase 22: テスト

### 22.1 ユニットテスト
- ⬜ **M151** Amount型テスト
- ⬜ **M152** Account Aggregateテスト
- ⬜ **M153** User Aggregateテスト
- ⬜ **M154** イベントシリアライズテスト

### 22.2 インテグレーションテスト
- ⬜ **M155** イベントストアテスト
- ⬜ **M156** 送金E2Eテスト
- ⬜ **M157** ATP発行E2Eテスト
- ⬜ **M158** 冪等性テスト
- ⬜ **M159** 同時実行（競合）テスト

### 22.3 負荷テスト
- ⬜ **M160** 100万イベント挿入テスト
- ⬜ **M161** 同時送金負荷テスト

---

## Phase 23: ドキュメント

### 23.1 開発者ドキュメント
- ⬜ **M162** APIドキュメント（OpenAPI/Swagger）
- ⬜ **M163** ローカル開発セットアップガイド
- ⬜ **M164** デプロイメントガイド

---

## 依存関係グラフ

```
Phase 1-7 (DB) ─────────────────────────────────────────┐
                                                        │
Phase 8 (Rust基盤) ─────────────────────────────────────┤
                                                        │
Phase 9 (ドメインモデル) ──┬────────────────────────────┤
                          │                            │
Phase 10 (Aggregate) ─────┼────────────────────────────┤
                          │                            │
Phase 11 (EventStore) ────┴─────┬──────────────────────┤
                                │                      │
Phase 12 (Projection) ──────────┤                      │
                                │                      │
Phase 13 (冪等性) ──────────────┤                      │
                                │                      │
Phase 14 (CommandHandler) ──────┴─────┬────────────────┤
                                      │                │
Phase 15 (認証) ──────────────────────┤                │
                                      │                │
Phase 16 (API) ───────────────────────┴────────────────┤
                                                       │
Phase 17-18 (エラー・ルーター) ────────────────────────┤
                                                       │
Phase 19-21 (運用) ────────────────────────────────────┤
                                                       │
Phase 22-23 (テスト・ドキュメント) ────────────────────┘
```

---

## 統計

- **総モジュール数**: 164
- **Phase 数**: 23
- **推定総工数**: 約80-120時間

---

## 優先度

1. **最優先**: Phase 1-7（DB）, Phase 8-11（Rust基盤〜EventStore）
2. **高優先**: Phase 12-14（Projection〜CommandHandler）
3. **中優先**: Phase 15-18（API層）
4. **低優先**: Phase 19-23（運用・テスト）
