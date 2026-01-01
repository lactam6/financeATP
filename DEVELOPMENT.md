# financeATP ローカル開発ガイド

## 必要なツール

| ツール     | バージョン | 用途             |
| ---------- | ---------- | ---------------- |
| Rust       | 1.75+      | ビルド・実行     |
| PostgreSQL | 14+        | データベース     |
| SQLx CLI   | 0.7+       | マイグレーション |

## セットアップ手順

### 1. リポジトリクローン

```bash
git clone <repository-url>
cd financeATP
```

### 2. PostgreSQLセットアップ

```bash
# データベース作成
createdb finance_atp

# 拡張機能インストール（PostgreSQL内で実行）
psql -d finance_atp -c "CREATE EXTENSION IF NOT EXISTS \"uuid-ossp\";"
psql -d finance_atp -c "CREATE EXTENSION IF NOT EXISTS \"pgcrypto\";"
```

### 3. 環境変数設定

`.env`ファイルを作成:

```bash
cp .env.example .env
```

編集して以下を設定:

```env
DATABASE_URL=postgres://postgres:password@localhost:5432/finance_atp
HOST=127.0.0.1
PORT=3000
```

### 4. マイグレーション実行

```bash
# SQLx CLIインストール（初回のみ）
cargo install sqlx-cli --no-default-features --features postgres

# マイグレーション実行
cd migrations
for f in *.sql; do psql -d finance_atp -f "$f"; done
```

### 5. ビルド・実行

```bash
# 開発ビルド
cargo build

# 実行
cargo run

# テスト実行
cargo test -- --test-threads=1
```

## ディレクトリ構造

```
financeATP/
├── src/
│   ├── main.rs           # エントリーポイント
│   ├── lib.rs            # ライブラリクレート
│   ├── api/              # HTTPエンドポイント
│   ├── domain/           # ドメインモデル
│   ├── aggregate/        # Aggregate（Account, User）
│   ├── event_store/      # イベントストア
│   ├── handlers/         # コマンドハンドラー
│   └── projection/       # 読み取りモデル
├── migrations/           # SQLマイグレーション
├── tests/                # 統合テスト
└── docs/                 # ドキュメント
```

## テスト

```bash
# 全テスト実行
cargo test -- --test-threads=1

# 特定のテストのみ
cargo test test_transfer_e2e -- --nocapture

# 負荷テスト
cargo run --bin load_test --release -- --events 1000
```

## トラブルシューティング

### データベース接続エラー

```bash
# PostgreSQLが起動しているか確認
pg_isready -h localhost -p 5432

# 接続テスト
psql -d finance_atp -c "SELECT 1"
```

### マイグレーションエラー

```bash
# テーブルをリセットして再実行
psql -d finance_atp -c "DROP SCHEMA public CASCADE; CREATE SCHEMA public;"
```
