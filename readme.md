# financeATP

ATP通貨を管理する堅牢なバックエンドAPI

## 概要

financeATPは、独自通貨「ATP」の発行・管理・送受信を行う**内部バックエンドAPI**です。

- **Rust + Axum + PostgreSQL** で構築
- **イベントソーシング** による完全な監査証跡
- **複式簿記** による堅牢な金融トランザクション処理

> ⚠️ このAPIは**内部サービス専用**です。認証済みのフロントエンド経由でアクセスしてください。

---

## クイックスタート

詳細は [QUICKSTART.md](QUICKSTART.md) を参照

```bash
# Dockerイメージのインポート
docker load -i finance_atp.tar

# 起動 (Windows)
start.bat

# 起動 (Mac/Linux)
./start.sh
```

---

## 認証

すべてのAPIリクエストには `X-API-Key` ヘッダーが必要です。

```bash
curl -H "X-API-Key: YOUR_API_KEY" http://localhost:3000/api/v1/...
```

### 開発用APIキー
```
test1234567890abcdef
```

---

## エンドポイント一覧

### ヘルスチェック

| メソッド | パス      | 説明             | 認証 |
| -------- | --------- | ---------------- | ---- |
| GET      | `/health` | サーバー稼働確認 | 不要 |

---

### ユーザー管理

| メソッド | パス                              | 説明             | 必要権限        |
| -------- | --------------------------------- | ---------------- | --------------- |
| POST     | `/api/v1/users`                   | ユーザー作成     | `write:users`   |
| GET      | `/api/v1/users/{user_id}`         | ユーザー情報取得 | `read:users`    |
| PATCH    | `/api/v1/users/{user_id}`         | ユーザー更新     | `write:users`   |
| DELETE   | `/api/v1/users/{user_id}`         | ユーザー無効化   | `write:users`   |
| GET      | `/api/v1/users/{user_id}/balance` | 残高取得         | `read:accounts` |

#### ユーザー作成例
```bash
curl -X POST http://localhost:3000/api/v1/users \
  -H "Content-Type: application/json" \
  -H "X-API-Key: test1234567890abcdef" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{
    "user_id": "550e8400-e29b-41d4-a716-446655440000",
    "username": "testuser",
    "email": "test@example.com"
  }'
```

---

### 送金

| メソッド | パス                | 説明     | 必要権限          |
| -------- | ------------------- | -------- | ----------------- |
| POST     | `/api/v1/transfers` | 送金実行 | `write:transfers` |

```bash
curl -X POST http://localhost:3000/api/v1/transfers \
  -H "Content-Type: application/json" \
  -H "X-API-Key: test1234567890abcdef" \
  -H "Idempotency-Key: $(uuidgen)" \
  -H "X-Request-User-Id: FROM_USER_ID" \
  -d '{
    "from_user_id": "FROM_USER_ID",
    "to_user_id": "TO_USER_ID",
    "amount": "100.00000000"
  }'
```

---

### 管理者API

| メソッド | パス                   | 説明             | 必要権限       |
| -------- | ---------------------- | ---------------- | -------------- |
| POST     | `/api/v1/admin/mint`   | ATP発行          | `admin:mint`   |
| POST     | `/api/v1/admin/burn`   | ATP焼却          | `admin:burn`   |
| GET      | `/api/v1/admin/events` | イベントログ取得 | `admin:events` |

#### ATP発行 (Mint)
```bash
curl -X POST http://localhost:3000/api/v1/admin/mint \
  -H "Content-Type: application/json" \
  -H "X-API-Key: test1234567890abcdef" \
  -H "Idempotency-Key: $(uuidgen)" \
  -d '{
    "recipient_user_id": "USER_ID",
    "amount": "1000.00000000",
    "reason": "Initial allocation"
  }'
```

---

### APIキー管理

| メソッド | パス                              | 説明          | 必要権限         |
| -------- | --------------------------------- | ------------- | ---------------- |
| POST     | `/api/v1/admin/api-keys`          | APIキー発行   | `admin:api-keys` |
| GET      | `/api/v1/admin/api-keys`          | APIキー一覧   | `admin:api-keys` |
| PATCH    | `/api/v1/admin/api-keys/{key_id}` | APIキー更新   | `admin:api-keys` |
| DELETE   | `/api/v1/admin/api-keys/{key_id}` | APIキー無効化 | `admin:api-keys` |

---

## 権限一覧

| 権限              | 説明                   |
| ----------------- | ---------------------- |
| `read:users`      | ユーザー情報の読み取り |
| `write:users`     | ユーザーの作成・更新   |
| `read:accounts`   | 口座情報の読み取り     |
| `write:transfers` | 送金の実行             |
| `admin:mint`      | ATPの発行              |
| `admin:burn`      | ATPの焼却              |
| `admin:events`    | イベントログの参照     |
| `admin:api-keys`  | APIキーの管理          |

---

## 冪等性 (Idempotency)

送金やMint/Burnなどの変更操作には `Idempotency-Key` ヘッダーが必要です。

```bash
-H "Idempotency-Key: 550e8400-e29b-41d4-a716-446655440000"
```

同じキーで複数回リクエストしても、処理は1回のみ実行されます。

---

## エラーレスポンス

```json
{
  "error": "エラーコード",
  "message": "詳細メッセージ"
}
```

| ステータス | 説明                      |
| ---------- | ------------------------- |
| 400        | リクエスト不正 / 残高不足 |
| 401        | 認証エラー                |
| 403        | 権限不足                  |
| 404        | リソースが見つからない    |
| 409        | 冪等性キー競合            |
| 500        | サーバーエラー            |

---

## 配布物

| ファイル             | 説明                    |
| -------------------- | ----------------------- |
| `finance_atp.tar`    | Dockerイメージ          |
| `docker-compose.yml` | コンテナ構成            |
| `start.bat`          | Windows起動スクリプト   |
| `start.sh`           | Mac/Linux起動スクリプト |
| `QUICKSTART.md`      | クイックスタートガイド  |
| `readme.md`          | 本ドキュメント          |

---

## ライセンス

MIT
