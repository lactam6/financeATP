# financeATP

ATP通貨を管理する堅牢なバックエンドAPI

## 概要

financeATPは、独自通貨「ATP」の発行・管理・送受信を行う**内部バックエンドAPI**です。

- **Rust + Axum + PostgreSQL** で構築
- **イベントソーシング** による完全な監査証跡
- **複式簿記** による堅牢な金融トランザクション処理

> ⚠️ このAPIは**内部サービス専用**です。認証済みのフロントエンド（Next.js等）経由でアクセスしてください。

## アーキテクチャ

```
Browser → Next.js (認証) → financeATP (金融処理) → PostgreSQL
```

## 技術スタック

| 項目           | 技術           |
| -------------- | -------------- |
| 言語           | Rust 1.75+     |
| フレームワーク | Axum           |
| データベース   | PostgreSQL 14+ |
| ORM            | SQLx           |

## クイックスタート

```bash
# 環境変数設定
cp .env.example .env

# ビルド
cargo build

# テスト
cargo test -- --test-threads=1

# 実行
cargo run
```

## Dockerでの起動（推奨）

```bash
# 全システムの起動
docker-compose up -d

# 停止
docker-compose down
```

## API エンドポイント

| メソッド | パス                        | 説明         |
| -------- | --------------------------- | ------------ |
| POST     | `/api/v1/users`             | ユーザー作成 |
| GET      | `/api/v1/users/:id/balance` | 残高取得     |
| POST     | `/api/v1/transfers`         | 送金         |
| POST     | `/api/v1/admin/mint`        | ATP発行      |
| POST     | `/api/v1/admin/burn`        | ATP焼却      |

## ドキュメント

- [OpenAPI仕様](docs/openapi.yaml)
- [ローカル開発ガイド](docs/DEVELOPMENT.md)
- [デプロイメントガイド](docs/DEPLOYMENT.md)

## ライセンス

MIT
