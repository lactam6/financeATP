# financeATP デプロイメントガイド

## 本番環境要件

| 項目     | 推奨スペック             |
| -------- | ------------------------ |
| CPU      | 4コア以上                |
| メモリ   | 8GB以上                  |
| ディスク | SSD 100GB以上            |
| OS       | Linux (Ubuntu 22.04 LTS) |

## 環境変数

| 変数名                     | 必須 | 説明                           |
| -------------------------- | ---- | ------------------------------ |
| `DATABASE_URL`             | ✅    | PostgreSQL接続文字列           |
| `HOST`                     | ✅    | バインドアドレス               |
| `PORT`                     | ✅    | リッスンポート                 |
| `DATABASE_MAX_CONNECTIONS` | -    | 最大接続数（デフォルト: 10）   |
| `RUST_LOG`                 | -    | ログレベル（デフォルト: info） |

## Docker Compose

```yaml
version: '3.8'

services:
  app:
    build: .
    ports:
      - "3000:3000"
    environment:
      - DATABASE_URL=postgres://postgres:password@db:5432/finance_atp
      - HOST=0.0.0.0
      - PORT=3000
    depends_on:
      db:
        condition: service_healthy

  db:
    image: postgres:14-alpine
    volumes:
      - pgdata:/var/lib/postgresql/data
      - ./migrations:/docker-entrypoint-initdb.d
    environment:
      - POSTGRES_DB=finance_atp
      - POSTGRES_PASSWORD=password
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U postgres"]
      interval: 5s
      timeout: 5s
      retries: 5

volumes:
  pgdata:
```

## Dockerfile

```dockerfile
FROM rust:1.75-slim as builder
WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/finance_atp /usr/local/bin/
ENTRYPOINT ["finance_atp"]
```

## PostgreSQL本番設定

### WALアーカイブ

```conf
# postgresql.conf
wal_level = replica
archive_mode = on
archive_command = 'cp %p /var/lib/postgresql/wal_archive/%f'
```

### バックアップ

```bash
# 日次フルバックアップ
pg_dump -Fc finance_atp > backup_$(date +%Y%m%d).dump

# リストア
pg_restore -d finance_atp backup_20260101.dump
```

## ヘルスチェック

```bash
# エンドポイント
GET /health

# 監視コマンド
curl -f http://localhost:3000/health || exit 1
```

## セキュリティ考慮事項

1. **APIキー管理**: 環境変数またはシークレット管理サービスで管理
2. **TLS**: リバースプロキシ（nginx）でTLS終端
3. **ネットワーク**: VPC/プライベートネットワーク内に配置
4. **ログ**: APIキーをマスク化してログ出力

## 監視項目

| メトリクス     | 閾値        | アラート |
| -------------- | ----------- | -------- |
| レスポンス時間 | p99 < 500ms | 警告     |
| エラー率       | < 0.1%      | 警告     |
| DB接続数       | < 80%       | 警告     |
| ディスク使用率 | < 80%       | 警告     |
