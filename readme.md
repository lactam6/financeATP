# financeATP

通貨「ATP」を管理する堅牢なバックエンドAPIサーバー

## 概要

financeATPは、独自通貨「ATP」の発行・管理・送受信を行うための**内部バックエンドAPI**です。
Rust + Axum + PostgreSQLで構築され、**イベントソーシング**と**複式簿記**により、
高い監査性・追跡可能性・障害復旧性を持つ金融トランザクション処理を実現します。

> **⚠️ 重要**: このAPIは**内部サービス専用**です。インターネットに直接公開せず、
> 必ず認証済みのフロントエンドサービス（Next.js等）経由でアクセスしてください。

---

## アーキテクチャ

```
┌─────────────┐     ┌─────────────────────┐     ┌──────────────────┐
│   Browser   │────▶│   Next.js Service   │────▶│   financeATP     │
│             │     │   (認証・トークン)   │     │   (Rust API)     │
└─────────────┘     └─────────────────────┘     └──────────────────┘
                            │                          │
                    ユーザー認証を担当          金融処理を担当
                    JWTトークン発行/検証        ATP残高管理/送金
                            │                          │
                            └──── APIキー認証 ─────────┘
                            └──── X-Request-User-Id ───┘
```

### 責務の分離

| サービス       | 責務                                           |
| -------------- | ---------------------------------------------- |
| **Next.js**    | ユーザー認証、JWT発行/検証、パスワード管理、UI |
| **financeATP** | ATP残高管理、送金処理、イベント記録            |

> **注意**: パスワードのハッシュ化等の認証処理はNext.jsサービスの責務です。
> financeATPはパスワード情報を一切保持しません。

---

## 口座モデル

### 設計方針: 1ユーザー = 1口座

```
┌────────────────────────────────────────────────────────────────┐
│                        ユーザー視点                             │
│   「user_id で残高照会・送金ができる」                          │
│   （account_id の存在を意識しない）                             │
└────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌────────────────────────────────────────────────────────────────┐
│                        内部実装                                 │
│   users (1) ────── (1) accounts (type: user_wallet)            │
│   ※ 一般ユーザーは user_wallet のみ                            │
│   ※ システムユーザー（発行元、手数料）は別途存在               │
└────────────────────────────────────────────────────────────────┘
```

### なぜ accounts テーブルを維持するか？

1. **複式簿記のため**: ATP発行には「発行元」勘定科目が必要
2. **将来の拡張性**: 必要になった場合に複数口座に対応可能
3. **責務の分離**: ユーザー情報と金融情報を分離

### システムユーザー

複式簿記を成立させるため、以下のシステムユーザーが存在します：

| ユーザーID       | 用途           | 口座タイプ     |
| ---------------- | -------------- | -------------- |
| `SYSTEM_MINT`    | ATP発行元      | mint_source    |
| `SYSTEM_FEE`     | 手数料収入     | fee_income     |
| `SYSTEM_RESERVE` | システム準備金 | system_reserve |

---

## 設計原則

### 1. イベントソーシング（Event Sourcing）
**現在の状態**ではなく**発生した事実（イベント）**を保存します。
現在の状態はイベントのリプレイにより任意の時点で再構築可能です。

```
従来の設計:  balance = 1000 (現在の状態のみ)
Event Sourcing: 
  - AccountCreated { initial_balance: 0 }
  - MoneyCredited { amount: 500 }
  - MoneyCredited { amount: 600 }
  - MoneyDebited { amount: 100 }
  → リプレイ結果: balance = 1000
```

### 2. 不変性（Immutability）

> **🔴 重要**: `balance` カラムを直接 `UPDATE` する設計は禁止です。

すべての残高変更は **イベントのINSERT** によってのみ行われます。
`account_balances` テーブルはイベントから**投影（Projection）された読み取り専用のキャッシュ**であり、
イベントストアが正（Single Source of Truth）です。

```
❌ NG: UPDATE account_balances SET balance = balance - 100 WHERE ...
✅ OK: INSERT INTO events (event_type, event_data, ...) VALUES ('MoneyDebited', ...)
       → Projectionサービスがeventsを読み取り、account_balancesを更新
```

### 3. 複式簿記（Double-Entry Bookkeeping）
すべての金銭移動は**借方（Debit）**と**貸方（Credit）**の両方に記録されます。
`借方合計 = 貸方合計` は常に維持され、DBトリガーで強制されます。

**例: ATP発行（Mint）**
```
借方: Aliceのuser_wallet +1000 ATP
貸方: SYSTEM_MINTのmint_source -1000 ATP
→ 借方合計 = 貸方合計 ✓
```

**例: 送金（Transfer）**
```
借方: Bobのuser_wallet +100 ATP
貸方: Aliceのuser_wallet -100 ATP
→ 借方合計 = 貸方合計 ✓
```

### 4. ACID特性の完全保証
- **Atomicity**: 送金は**単一トランザクション**で完結（部分的成功なし）
- **Consistency**: DB制約とトリガーで整合性を強制
- **Isolation**: 楽観的ロック + リトライ戦略
- **Durability**: WALアーカイブで永続性を保証

### 5. 冪等性（Idempotency）
すべての書き込みAPIは `Idempotency-Key` を必須とし、二重処理を防止します。
タイムアウト処理により、処理中のまま残ったキーも適切にリセットされます。

### 6. 監査ログ（Audit Trail）
すべての操作は改ざん検知可能なハッシュチェーンで記録されます。
シーケンス番号と排他ロックにより、並行処理でもチェーンの一貫性を保証します。

---

## 技術スタック

| 項目                 | 技術           |
| -------------------- | -------------- |
| 言語                 | Rust 1.75+     |
| Webフレームワーク    | Axum           |
| データベース         | PostgreSQL 14+ |
| ORM/クエリビルダー   | SQLx           |
| 非同期ランタイム     | Tokio          |
| 金額型               | rust_decimal   |
| イベントシリアライズ | serde_json     |

---

## セキュリティ

### サービス間認証（APIキー）

```
Headers:
  X-API-Key: sk_live_xxxxxxxxxxxxxxxx
  X-Request-User-Id: user_abc123        # Next.jsが認証したユーザーID
  X-Correlation-Id: req_xyz789          # リクエスト追跡用
```

### 信頼モデル

```
1. ブラウザ → Next.js: ユーザーがJWTでログイン
2. Next.js: JWTを検証し、ユーザーIDを取得
3. Next.js → financeATP: APIキー + X-Request-User-Id でリクエスト
4. financeATP: APIキーを検証し、X-Request-User-Id を信頼して処理
5. financeATP: 送金時、X-Request-User-Id が送金元ユーザーと一致するか検証
```

### APIキーのログマスク化

APIキーがログに漏洩しないよう、すべてのログ出力時にマスク化を行います。

```rust
use axum::http::HeaderMap;
use std::collections::HashMap;

/// ログ出力時にAPIキーをマスク
pub fn mask_headers_for_logging(headers: &HeaderMap) -> HashMap<String, String> {
    headers.iter()
        .map(|(k, v)| {
            let key = k.as_str();
            let value = if key.eq_ignore_ascii_case("x-api-key") {
                // "sk_live_abc123..." → "sk_live_****"
                let val = v.to_str().unwrap_or("");
                if val.len() > 8 {
                    format!("{}****", &val[..8])
                } else {
                    "****".to_string()
                }
            } else {
                v.to_str().unwrap_or("").to_string()
            };
            (key.to_string(), value)
        })
        .collect()
}

// 使用例
tracing::info!(
    headers = ?mask_headers_for_logging(&request.headers()),
    "Incoming request"
);
```

### Rate Limiting

DoS攻撃を防ぐため、APIキーごとにリクエスト数を制限します。

---

## データベーススキーマ

### 拡張機能

```sql
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
```

---

### api_keys テーブル

```sql
CREATE TABLE api_keys (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    key_prefix VARCHAR(12) NOT NULL,
    key_hash VARCHAR(64) NOT NULL,
    permissions TEXT[] NOT NULL,
    allowed_ips INET[],
    rate_limit_per_minute INTEGER DEFAULT 1000,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    
    UNIQUE(key_prefix)
);

CREATE INDEX idx_api_keys_active ON api_keys(key_prefix) WHERE is_active = TRUE;
```

---

### rate_limit_buckets テーブル

```sql
CREATE TABLE rate_limit_buckets (
    api_key_id UUID NOT NULL REFERENCES api_keys(id),
    window_start TIMESTAMPTZ NOT NULL,
    request_count INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (api_key_id, window_start)
);

CREATE INDEX idx_rate_limit_expires ON rate_limit_buckets(window_start);

CREATE OR REPLACE FUNCTION check_and_increment_rate_limit(
    p_api_key_id UUID,
    p_limit INTEGER
) RETURNS BOOLEAN AS $$
DECLARE
    v_window TIMESTAMPTZ;
    v_count INTEGER;
BEGIN
    v_window := date_trunc('minute', NOW());
    
    INSERT INTO rate_limit_buckets (api_key_id, window_start, request_count)
    VALUES (p_api_key_id, v_window, 1)
    ON CONFLICT (api_key_id, window_start) 
    DO UPDATE SET request_count = rate_limit_buckets.request_count + 1
    RETURNING request_count INTO v_count;
    
    RETURN v_count <= p_limit;
END;
$$ LANGUAGE plpgsql;
```

---

### events テーブル（イベントストア）

**パーティション対応。100万件以上のデータでもパフォーマンスを維持。**

```sql
CREATE TABLE events (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    aggregate_type VARCHAR(50) NOT NULL,
    aggregate_id UUID NOT NULL,
    version BIGINT NOT NULL,
    event_type VARCHAR(100) NOT NULL,
    event_data JSONB NOT NULL,
    context JSONB NOT NULL DEFAULT '{}',
    idempotency_key UUID,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (id, created_at),
    CONSTRAINT unique_aggregate_version UNIQUE (aggregate_id, version),
    CONSTRAINT unique_idempotency UNIQUE (idempotency_key) 
        DEFERRABLE INITIALLY DEFERRED
) PARTITION BY RANGE (created_at);

-- 月別パーティション（自動作成を推奨）
CREATE TABLE events_2026_01 PARTITION OF events
    FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
CREATE TABLE events_2026_02 PARTITION OF events
    FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');

CREATE INDEX idx_events_aggregate ON events(aggregate_type, aggregate_id, version);
CREATE INDEX idx_events_type ON events(event_type, created_at);
CREATE INDEX idx_events_correlation ON events((context->>'correlation_id'));

-- イベントは削除・更新禁止（イミュータブル）
CREATE OR REPLACE FUNCTION prevent_event_modification() RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'DELETE' THEN
        RAISE EXCEPTION 'DELETE is not allowed on % table', TG_TABLE_NAME;
    ELSIF TG_OP = 'UPDATE' THEN
        RAISE EXCEPTION 'UPDATE is not allowed on % table', TG_TABLE_NAME;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER no_modify_events
    BEFORE UPDATE OR DELETE ON events
    FOR EACH ROW EXECUTE FUNCTION prevent_event_modification();
```

---

### event_snapshots テーブル

**パフォーマンス最適化。100イベントごとにスナップショットを作成。**

```sql
CREATE TABLE event_snapshots (
    aggregate_type VARCHAR(50) NOT NULL,
    aggregate_id UUID NOT NULL,
    version BIGINT NOT NULL,
    state JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (aggregate_type, aggregate_id)
);

CREATE INDEX idx_snapshots_version ON event_snapshots(aggregate_id, version);
```

**スナップショット作成ポリシー:**
- **作成タイミング**: Aggregateバージョンが100の倍数になったとき
- **保持**: 最新のスナップショットのみ保持（UPSERT）
- **効果**: 100件以上のイベントを持つAggregateの読み込みを高速化

---

### users テーブル

```sql
CREATE TABLE users (
    id UUID PRIMARY KEY,
    username VARCHAR(50) UNIQUE NOT NULL,
    email VARCHAR(100) UNIQUE NOT NULL,
    display_name VARCHAR(100),
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    deleted_at TIMESTAMPTZ,
    
    CONSTRAINT valid_username CHECK (
        LENGTH(username) >= 3 AND 
        username ~ '^[a-zA-Z0-9_]+$'
    ),
    CONSTRAINT valid_email CHECK (
        email ~* '^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$'
    )
);

CREATE INDEX idx_users_active ON users(id) WHERE deleted_at IS NULL AND is_system = FALSE;
CREATE INDEX idx_users_email ON users(email) WHERE deleted_at IS NULL;

-- システムユーザーを事前作成
INSERT INTO users (id, username, email, display_name, is_system, created_at, updated_at) VALUES
    ('00000000-0000-0000-0000-000000000001', 'SYSTEM_MINT', 'mint@system.internal', 'ATP発行元', TRUE, NOW(), NOW()),
    ('00000000-0000-0000-0000-000000000002', 'SYSTEM_FEE', 'fee@system.internal', '手数料収入', TRUE, NOW(), NOW()),
    ('00000000-0000-0000-0000-000000000003', 'SYSTEM_RESERVE', 'reserve@system.internal', 'システム準備金', TRUE, NOW(), NOW());
```

---

### account_types テーブル

```sql
CREATE TABLE account_types (
    code VARCHAR(20) PRIMARY KEY,
    name VARCHAR(50) NOT NULL,
    is_debit_normal BOOLEAN NOT NULL,
    is_system_only BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO account_types (code, name, is_debit_normal, is_system_only) VALUES
    ('user_wallet', 'ユーザーウォレット', TRUE, FALSE),
    ('mint_source', 'ATP発行元', FALSE, TRUE),
    ('fee_income', '手数料収入', FALSE, TRUE),
    ('system_reserve', 'システム準備金', TRUE, TRUE);
```

---

### accounts テーブル

**内部テーブル。APIでは直接露出しない。**

```sql
CREATE TABLE accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE RESTRICT,
    account_type VARCHAR(20) NOT NULL REFERENCES account_types(code),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    UNIQUE(user_id, account_type),
    
    CONSTRAINT user_wallet_only CHECK (
        (SELECT is_system FROM users WHERE id = user_id) = TRUE 
        OR account_type = 'user_wallet'
    )
);

CREATE INDEX idx_accounts_user ON accounts(user_id);

CREATE OR REPLACE FUNCTION get_wallet_account_id(p_user_id UUID) 
RETURNS UUID AS $$
DECLARE
    v_account_id UUID;
BEGIN
    SELECT id INTO v_account_id
    FROM accounts
    WHERE user_id = p_user_id AND account_type = 'user_wallet';
    
    IF v_account_id IS NULL THEN
        RAISE EXCEPTION 'Wallet account not found for user %', p_user_id;
    END IF;
    
    RETURN v_account_id;
END;
$$ LANGUAGE plpgsql;

-- システムユーザーの口座を事前作成
INSERT INTO accounts (user_id, account_type) VALUES
    ('00000000-0000-0000-0000-000000000001', 'mint_source'),
    ('00000000-0000-0000-0000-000000000002', 'fee_income'),
    ('00000000-0000-0000-0000-000000000003', 'system_reserve');
```

---

### account_balances テーブル

**Projection（読み取り専用キャッシュ）。イベントから投影される。**

```sql
CREATE TABLE account_balances (
    account_id UUID PRIMARY KEY REFERENCES accounts(id),
    balance NUMERIC(20, 8) NOT NULL DEFAULT 0,
    last_event_id UUID NOT NULL,
    last_event_version BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT non_negative_balance CHECK (balance >= 0),
    CONSTRAINT max_balance CHECK (balance <= 1000000000000.00000000)
);

CREATE VIEW user_balances AS
SELECT 
    u.id as user_id,
    u.username,
    u.display_name,
    ab.balance,
    ab.updated_at
FROM users u
JOIN accounts a ON u.id = a.user_id AND a.account_type = 'user_wallet'
JOIN account_balances ab ON a.id = ab.account_id
WHERE u.is_system = FALSE AND u.deleted_at IS NULL;
```

---

### ledger_entries テーブル

```sql
CREATE TABLE ledger_entries (
    id UUID NOT NULL DEFAULT gen_random_uuid(),
    journal_id UUID NOT NULL,
    transfer_event_id UUID NOT NULL,
    account_id UUID NOT NULL REFERENCES accounts(id),
    amount NUMERIC(20, 8) NOT NULL,
    entry_type VARCHAR(6) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    PRIMARY KEY (id, created_at),
    CONSTRAINT positive_amount CHECK (amount > 0),
    CONSTRAINT valid_entry_type CHECK (entry_type IN ('debit', 'credit'))
) PARTITION BY RANGE (created_at);

CREATE INDEX idx_ledger_account ON ledger_entries(account_id);
CREATE INDEX idx_ledger_journal ON ledger_entries(journal_id);

CREATE TABLE ledger_entries_2026_01 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-01-01') TO ('2026-02-01');
CREATE TABLE ledger_entries_2026_02 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');
```

---

### 複式簿記バランスチェック

**STATEMENTレベルで一括チェック（N+1問題を回避）**

```sql
CREATE OR REPLACE FUNCTION check_ledger_balance_batch() RETURNS TRIGGER AS $$
DECLARE
    unbalanced RECORD;
BEGIN
    FOR unbalanced IN
        SELECT 
            journal_id,
            SUM(CASE WHEN entry_type = 'debit' THEN amount ELSE 0 END) as debit_sum,
            SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE 0 END) as credit_sum
        FROM ledger_entries
        WHERE journal_id IN (SELECT DISTINCT journal_id FROM inserted_entries)
        GROUP BY journal_id
        HAVING SUM(CASE WHEN entry_type = 'debit' THEN amount ELSE 0 END) !=
               SUM(CASE WHEN entry_type = 'credit' THEN amount ELSE 0 END)
    LOOP
        RAISE EXCEPTION 'Unbalanced ledger entry for journal %: debit=%, credit=%', 
            unbalanced.journal_id, unbalanced.debit_sum, unbalanced.credit_sum;
    END LOOP;
    
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE CONSTRAINT TRIGGER validate_ledger_balance
    AFTER INSERT ON ledger_entries
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH STATEMENT
    EXECUTE FUNCTION check_ledger_balance_batch();
```

---

### idempotency_keys テーブル

```sql
CREATE TABLE idempotency_keys (
    key UUID PRIMARY KEY,
    request_hash VARCHAR(64) NOT NULL,
    event_id UUID,
    response_status INTEGER,
    response_body JSONB,
    processing_status VARCHAR(20) NOT NULL DEFAULT 'pending',
    processing_started_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL DEFAULT NOW() + INTERVAL '24 hours'
);

CREATE INDEX idx_idempotency_expires ON idempotency_keys(expires_at);
CREATE INDEX idx_idempotency_processing ON idempotency_keys(processing_status, processing_started_at) 
    WHERE processing_status = 'processing';

-- タイムアウト処理（5分以上処理中のキーをリセット）
CREATE OR REPLACE FUNCTION reset_stale_idempotency_keys() RETURNS INTEGER AS $$
DECLARE
    affected INTEGER;
BEGIN
    UPDATE idempotency_keys
    SET processing_status = 'failed'
    WHERE processing_status = 'processing'
      AND processing_started_at < NOW() - INTERVAL '5 minutes';
    
    GET DIAGNOSTICS affected = ROW_COUNT;
    RETURN affected;
END;
$$ LANGUAGE plpgsql;
```

---

### audit_logs テーブル

```sql
CREATE TABLE audit_logs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    sequence_number BIGSERIAL NOT NULL,
    api_key_id UUID REFERENCES api_keys(id),
    request_user_id UUID,
    correlation_id UUID,
    action VARCHAR(50) NOT NULL,
    resource_type VARCHAR(50),
    resource_id UUID,
    before_state JSONB,
    after_state JSONB,
    changed_fields TEXT[],
    client_ip INET,
    previous_hash VARCHAR(64) NOT NULL,
    current_hash VARCHAR(64) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT unique_sequence UNIQUE (sequence_number)
);

CREATE INDEX idx_audit_user ON audit_logs(request_user_id, created_at);
CREATE INDEX idx_audit_action ON audit_logs(action, created_at);
CREATE INDEX idx_audit_correlation ON audit_logs(correlation_id);

-- ハッシュチェーン計算（排他ロックでレース条件を防止）
CREATE OR REPLACE FUNCTION calculate_audit_hash() RETURNS TRIGGER AS $$
DECLARE
    prev_hash VARCHAR(64);
    hash_input TEXT;
BEGIN
    PERFORM pg_advisory_xact_lock(hashtext('audit_logs_chain'));
    
    SELECT current_hash INTO prev_hash 
    FROM audit_logs 
    ORDER BY sequence_number DESC
    LIMIT 1;
    
    NEW.previous_hash := COALESCE(prev_hash, '0000000000000000000000000000000000000000000000000000000000000000');
    
    hash_input := NEW.id::text || 
                  NEW.sequence_number::text ||
                  NEW.action || 
                  COALESCE(NEW.request_user_id::text, '') ||
                  COALESCE(NEW.before_state::text, '') ||
                  COALESCE(NEW.after_state::text, '') ||
                  NEW.previous_hash;
    
    NEW.current_hash := encode(sha256(hash_input::bytea), 'hex');
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER hash_audit_log
    BEFORE INSERT ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION calculate_audit_hash();

CREATE TRIGGER no_modify_audit
    BEFORE UPDATE OR DELETE ON audit_logs
    FOR EACH ROW EXECUTE FUNCTION prevent_event_modification();
```

---

## API仕様

### 共通ヘッダー

```
X-API-Key: sk_live_xxxxxxxxxxxxxxxx     # 必須: サービス認証
X-Request-User-Id: user_abc123          # 推奨: 操作者のユーザーID（送金時は必須）
X-Correlation-Id: req_xyz789            # 推奨: リクエスト追跡用
Idempotency-Key: 550e8400-e29b-...      # 書き込み操作時に必須
Content-Type: application/json
```

### エンドポイント一覧

```
# ユーザー管理
POST   /users                    # ユーザー作成（口座も自動作成）
GET    /users/:user_id           # ユーザー情報取得
PATCH  /users/:user_id           # ユーザー情報更新
DELETE /users/:user_id           # ユーザー論理削除

# 残高・送金（user_id ベース）
GET    /users/:user_id/balance   # 残高取得
GET    /users/:user_id/history   # 取引履歴

POST   /transfers                # 送金実行（user_id を使用）
GET    /transfers/:transfer_id   # 送金詳細

# 管理API
POST   /admin/mint               # ATP発行（user_id に発行）
POST   /admin/burn               # ATP焼却
GET    /admin/events             # イベントストリーム取得
```

---

### POST /users

**ユーザー作成（user_wallet 口座も自動作成）**

```
Headers:
  X-API-Key: sk_live_xxx
  Idempotency-Key: 550e8400-e29b-41d4-a716-446655440000

Request:
{
  "user_id": "123e4567-e89b-12d3-a456-426614174000",
  "username": "alice",
  "email": "alice@example.com",
  "display_name": "Alice Smith"
}

Response (201 Created):
{
  "user_id": "123e4567-e89b-12d3-a456-426614174000",
  "username": "alice",
  "email": "alice@example.com",
  "display_name": "Alice Smith",
  "balance": "0.00000000",
  "created_at": "2026-01-01T15:46:00Z"
}
```

---

### GET /users/:user_id/balance

**残高取得（account_id は内部で自動解決）**

```
Headers:
  X-API-Key: sk_live_xxx

Response (200 OK):
{
  "user_id": "123e4567-e89b-12d3-a456-426614174000",
  "username": "alice",
  "balance": "1500.00000000",
  "updated_at": "2026-01-01T15:45:00Z"
}
```

---

### POST /transfers

**送金実行（user_id を使用）**

> **注意**: `X-Request-User-Id` が `from_user_id` と一致しない場合、403 Forbidden

```
Headers:
  X-API-Key: sk_live_xxx
  X-Request-User-Id: user_abc123
  Idempotency-Key: 550e8400-e29b-41d4-a716-446655440000

Request:
{
  "from_user_id": "abc12345-e89b-12d3-a456-426614174000",
  "to_user_id": "def67890-e89b-12d3-a456-426614174000",
  "amount": "100.00000000",
  "memo": "お支払いありがとうございます"
}

Response (201 Created):
{
  "transfer_id": "789e0123-e89b-12d3-a456-426614174000",
  "status": "completed",
  "from_user_id": "abc12345-e89b-12d3-a456-426614174000",
  "to_user_id": "def67890-e89b-12d3-a456-426614174000",
  "amount": "100.00000000",
  "created_at": "2026-01-01T15:46:00Z"
}
```

---

### POST /admin/mint

**ATP発行（SYSTEM_MINT から指定ユーザーへ）**

```
Headers:
  X-API-Key: sk_live_xxx (admin権限必要)
  Idempotency-Key: 550e8400-e29b-41d4-a716-446655440000

Request:
{
  "to_user_id": "123e4567-e89b-12d3-a456-426614174000",
  "amount": "1000.00000000",
  "reason": "初期残高付与"
}

Response (201 Created):
{
  "mint_id": "789e0123-e89b-12d3-a456-426614174000",
  "status": "completed",
  "to_user_id": "123e4567-e89b-12d3-a456-426614174000",
  "amount": "1000.00000000",
  "created_at": "2026-01-01T15:46:00Z"
}

# 内部処理（複式簿記）:
#   借方: Aliceのuser_wallet +1000
#   貸方: SYSTEM_MINTのmint_source -1000
```

---

### エラーレスポンス

| Status | Error                 | 説明                           |
| ------ | --------------------- | ------------------------------ |
| 400    | invalid_request       | リクエスト形式が不正           |
| 400    | insufficient_balance  | 残高不足                       |
| 400    | account_frozen        | アカウント凍結中               |
| 401    | invalid_api_key       | APIキーが無効                  |
| 403    | permission_denied     | 権限不足                       |
| 403    | unauthorized_transfer | 送金元ユーザーが本人でない     |
| 404    | user_not_found        | ユーザーが存在しない           |
| 409    | idempotency_conflict  | 同一キーで異なるリクエスト     |
| 409    | version_conflict      | 同時更新の競合（リトライ推奨） |
| 429    | rate_limit_exceeded   | レート制限超過                 |

---

## Rust実装

### Amount型（ビジネスルール強制）

金額は `rust_decimal` を使用し、型レベルでビジネスルールを強制します。

```rust
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 金額を表すドメインプリミティブ
/// 生成時にビジネスルールを検証し、不正な値の存在を型レベルで防止
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::Type)]
#[sqlx(transparent)]
pub struct Amount(Decimal);

#[derive(Debug, Error)]
pub enum AmountError {
    #[error("金額は0より大きい必要があります")]
    NotPositive,
    #[error("小数点以下は8桁までです")]
    TooManyDecimals,
    #[error("金額が大きすぎます（最大: 1兆ATP）")]
    Overflow,
}

impl Amount {
    /// 新しいAmountを作成（ビジネスルール検証付き）
    pub fn new(value: Decimal) -> Result<Self, AmountError> {
        // ルール1: 0より大きい
        if value <= Decimal::ZERO {
            return Err(AmountError::NotPositive);
        }
        // ルール2: 小数点以下8桁まで
        if value.scale() > 8 {
            return Err(AmountError::TooManyDecimals);
        }
        // ルール3: 最大1兆ATP
        let max = Decimal::from(1_000_000_000_000i64);
        if value > max {
            return Err(AmountError::Overflow);
        }
        Ok(Self(value))
    }

    pub fn value(&self) -> Decimal {
        self.0
    }
    
    /// 金額の加算（オーバーフローチェック付き）
    pub fn try_add(&self, other: &Amount) -> Result<Amount, AmountError> {
        Amount::new(self.0 + other.0)
    }
}

// Amountは直接構築できないため、必ずnew()を経由する
// → 不正な金額は存在し得ない
```

---

### イベントストア（アトミック複数Aggregate対応）

送金など複数Aggregateを同時に更新する場合も、**単一トランザクション**で完結させます。

```rust
pub struct EventStore {
    pool: PgPool,
}

/// 複数AggregateへのイベントをアトミックにCommitするための構造体
pub struct AggregateOperation {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub expected_version: i64,
    pub events: Vec<serde_json::Value>,
}

impl EventStore {
    /// 複数のAggregateにイベントをアトミックに保存
    /// 送金など、複数のAggregateを同時に更新する必要がある場合に使用
    pub async fn append_atomic(
        &self,
        operations: Vec<AggregateOperation>,
        idempotency_key: Option<Uuid>,
        context: &OperationContext,
    ) -> Result<Vec<Uuid>, EventStoreError> {
        const MAX_RETRIES: u32 = 3;
        
        for attempt in 0..MAX_RETRIES {
            match self.try_append_atomic(&operations, idempotency_key, context).await {
                Ok(ids) => return Ok(ids),
                Err(EventStoreError::ConcurrencyConflict { .. }) if attempt < MAX_RETRIES - 1 => {
                    // リトライ前に少し待機（指数バックオフ）
                    tokio::time::sleep(Duration::from_millis(50 * 2u64.pow(attempt))).await;
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        
        Err(EventStoreError::MaxRetriesExceeded)
    }
    
    async fn try_append_atomic(
        &self,
        operations: &[AggregateOperation],
        idempotency_key: Option<Uuid>,
        context: &OperationContext,
    ) -> Result<Vec<Uuid>, EventStoreError> {
        // 単一トランザクションで開始
        let mut tx = self.pool.begin().await?;
        
        let mut event_ids = Vec::new();
        let mut first_event = true;
        
        for op in operations {
            // 楽観的ロック: 現在のバージョンを確認
            let current_version: Option<i64> = sqlx::query_scalar(
                "SELECT MAX(version) FROM events WHERE aggregate_id = $1 FOR UPDATE"
            )
            .bind(op.aggregate_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
            
            let current = current_version.unwrap_or(-1);
            if current != op.expected_version {
                // ロールバック（明示的に不要だが明確化のため）
                tx.rollback().await?;
                return Err(EventStoreError::ConcurrencyConflict {
                    aggregate_id: op.aggregate_id,
                    expected: op.expected_version,
                    actual: current,
                });
            }
            
            // イベントを挿入
            for (i, event) in op.events.iter().enumerate() {
                let event_id = Uuid::new_v4();
                let version = op.expected_version + 1 + i as i64;
                
                sqlx::query(
                    r#"
                    INSERT INTO events (
                        id, aggregate_type, aggregate_id, version,
                        event_type, event_data, context, idempotency_key
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#
                )
                .bind(event_id)
                .bind(&op.aggregate_type)
                .bind(op.aggregate_id)
                .bind(version)
                .bind(get_event_type(event))
                .bind(event)
                .bind(serde_json::to_value(context)?)
                .bind(if first_event { idempotency_key } else { None })
                .execute(&mut *tx)
                .await?;
                
                event_ids.push(event_id);
                first_event = false;
            }
        }
        
        // すべて成功した場合のみコミット
        tx.commit().await?;
        
        Ok(event_ids)
    }
    
    /// スナップショットからAggregateをロード
    /// 100イベントごとにスナップショットを作成するため、高速
    pub async fn load_aggregate<A: Aggregate + DeserializeOwned + Default>(
        &self,
        aggregate_id: Uuid,
    ) -> Result<Option<A>, EventStoreError> {
        // 1. スナップショットを取得
        let snapshot: Option<(i64, serde_json::Value)> = sqlx::query_as(
            "SELECT version, state FROM event_snapshots WHERE aggregate_type = $1 AND aggregate_id = $2"
        )
        .bind(A::aggregate_type())
        .bind(aggregate_id)
        .fetch_optional(&self.pool)
        .await?;
        
        let (from_version, initial_state) = match snapshot {
            Some((v, s)) => (v, Some(serde_json::from_value::<A>(s)?)),
            None => (-1, None),
        };
        
        // 2. スナップショット以降のイベントのみ取得
        let events: Vec<StoredEvent> = sqlx::query_as(
            "SELECT * FROM events WHERE aggregate_id = $1 AND version > $2 ORDER BY version"
        )
        .bind(aggregate_id)
        .bind(from_version)
        .fetch_all(&self.pool)
        .await?;
        
        if events.is_empty() && initial_state.is_none() {
            return Ok(None);
        }
        
        // 3. イベントを適用
        let aggregate = events.into_iter().fold(
            initial_state.unwrap_or_default(),
            |agg, event| agg.apply_stored(event),
        );
        
        Ok(Some(aggregate))
    }
    
    /// スナップショットを保存（100イベントごと）
    pub async fn save_snapshot_if_needed<A: Aggregate + Serialize>(
        &self,
        aggregate: &A,
    ) -> Result<(), EventStoreError> {
        const SNAPSHOT_INTERVAL: i64 = 100;
        
        if aggregate.version() > 0 && aggregate.version() % SNAPSHOT_INTERVAL == 0 {
            sqlx::query(
                r#"
                INSERT INTO event_snapshots (aggregate_type, aggregate_id, version, state)
                VALUES ($1, $2, $3, $4)
                ON CONFLICT (aggregate_type, aggregate_id) 
                DO UPDATE SET version = $3, state = $4, created_at = NOW()
                "#
            )
            .bind(A::aggregate_type())
            .bind(aggregate.id())
            .bind(aggregate.version())
            .bind(serde_json::to_value(aggregate)?)
            .execute(&self.pool)
            .await?;
        }
        
        Ok(())
    }
}
```

---

### Projection更新サービス

イベント保存後、**Projection（account_balances, ledger_entries）を更新**します。

```rust
pub struct ProjectionService {
    pool: PgPool,
}

impl ProjectionService {
    /// 送金後のProjection更新
    pub async fn apply_transfer(
        &self,
        journal_id: Uuid,
        event_ids: &[Uuid],
        from_account_id: Uuid,
        to_account_id: Uuid,
        amount: Decimal,
    ) -> Result<(), ProjectionError> {
        let mut tx = self.pool.begin().await?;
        
        // 1. account_balances 更新（イベントから投影）
        sqlx::query(
            "UPDATE account_balances SET balance = balance - $1, last_event_id = $2, updated_at = NOW() WHERE account_id = $3"
        )
        .bind(amount)
        .bind(event_ids.first())
        .bind(from_account_id)
        .execute(&mut *tx)
        .await?;
        
        sqlx::query(
            "UPDATE account_balances SET balance = balance + $1, last_event_id = $2, updated_at = NOW() WHERE account_id = $3"
        )
        .bind(amount)
        .bind(event_ids.get(1))
        .bind(to_account_id)
        .execute(&mut *tx)
        .await?;
        
        // 2. ledger_entries 作成（複式簿記）
        sqlx::query(
            "INSERT INTO ledger_entries (journal_id, transfer_event_id, account_id, amount, entry_type) VALUES ($1, $2, $3, $4, 'credit')"
        )
        .bind(journal_id)
        .bind(event_ids.first())
        .bind(from_account_id)
        .bind(amount)
        .execute(&mut *tx)
        .await?;
        
        sqlx::query(
            "INSERT INTO ledger_entries (journal_id, transfer_event_id, account_id, amount, entry_type) VALUES ($1, $2, $3, $4, 'debit')"
        )
        .bind(journal_id)
        .bind(event_ids.get(1))
        .bind(to_account_id)
        .bind(amount)
        .execute(&mut *tx)
        .await?;
        
        tx.commit().await?;
        
        Ok(())
    }
}
```

---

### 送金コマンドハンドラ

```rust
pub struct TransferCommand {
    pub idempotency_key: Uuid,
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub amount: Amount,  // ← ビジネスルール検証済み
    pub memo: Option<String>,
}

impl TransferCommandHandler {
    pub async fn execute(
        &self,
        cmd: TransferCommand,
        context: OperationContext,
    ) -> Result<TransferResult, TransferError> {
        // 1. 認可チェック: X-Request-User-Id == from_user_id
        if Some(cmd.from_user_id) != context.request_user_id {
            return Err(TransferError::UnauthorizedTransfer);
        }
        
        // 2. user_id → account_id に変換
        let from_account_id = self.get_wallet_account_id(cmd.from_user_id).await?;
        let to_account_id = self.get_wallet_account_id(cmd.to_user_id).await?;
        
        // 3. Aggregateをロード
        let from_account = self.event_store.load_aggregate::<Account>(from_account_id).await?
            .ok_or(TransferError::AccountNotFound)?;
        let to_account = self.event_store.load_aggregate::<Account>(to_account_id).await?
            .ok_or(TransferError::AccountNotFound)?;
        
        // 4. イベント生成（ビジネスルール検証）
        let transfer_id = Uuid::new_v4();
        let description = cmd.memo.unwrap_or_else(|| "Transfer".to_string());
        
        let debit_event = from_account.debit(cmd.amount.value(), transfer_id, description.clone())?;
        let credit_event = to_account.credit(cmd.amount.value(), transfer_id, description)?;
        
        // 5. アトミックにイベント保存（単一トランザクション）
        let operations = vec![
            AggregateOperation {
                aggregate_type: "Account".to_string(),
                aggregate_id: from_account_id,
                expected_version: from_account.version(),
                events: vec![serde_json::to_value(&debit_event)?],
            },
            AggregateOperation {
                aggregate_type: "Account".to_string(),
                aggregate_id: to_account_id,
                expected_version: to_account.version(),
                events: vec![serde_json::to_value(&credit_event)?],
            },
        ];
        
        let event_ids = self.event_store.append_atomic(
            operations,
            Some(cmd.idempotency_key),
            &context,
        ).await?;
        
        // 6. Projection更新
        let journal_id = Uuid::new_v4();
        self.projection_service.apply_transfer(
            journal_id,
            &event_ids,
            from_account_id,
            to_account_id,
            cmd.amount.value(),
        ).await?;
        
        // 7. スナップショット作成（必要な場合）
        let updated_from = from_account.apply(debit_event);
        let updated_to = to_account.apply(credit_event);
        self.event_store.save_snapshot_if_needed(&updated_from).await?;
        self.event_store.save_snapshot_if_needed(&updated_to).await?;
        
        Ok(TransferResult {
            transfer_id,
            status: "completed".to_string(),
            amount: cmd.amount.value(),
        })
    }
}
```

---

## バックアップ戦略

### 1. WALアーカイブ（継続的バックアップ）

PostgreSQLのWAL（Write-Ahead Log）を継続的にアーカイブし、
任意の時点への復旧（PITR: Point-In-Time Recovery）を可能にします。

```ini
# postgresql.conf
wal_level = replica
archive_mode = on
archive_command = 'aws s3 cp %p s3://financeATP-backup/wal/%f --sse AES256'
archive_timeout = 60
```

### 2. 日次フルバックアップ

```bash
# crontab（毎日午前3時）
0 3 * * * pg_basebackup -D /backup/$(date +%Y%m%d) -Ft -z -P
```

### 3. バックアップ保持ポリシー

| 種類                 | 保持期間 |
| -------------------- | -------- |
| WALアーカイブ        | 7日間    |
| 日次フルバックアップ | 30日間   |
| 月次フルバックアップ | 1年間    |

### 4. 復旧テスト

月に1回、本番バックアップをテスト環境に復元して検証します。

### 5. イベントソーシングによる追加の安全性

イベントストアはイミュータブルなため、任意の時点の状態を
イベントリプレイで再構築できます（DBバックアップに加えた追加の保険）。

---

## 定期メンテナンスジョブ

```sql
-- 1. 古いRate Limitバケットの削除（5分ごと）
DELETE FROM rate_limit_buckets WHERE window_start < NOW() - INTERVAL '5 minutes';

-- 2. タイムアウトした冪等性キーのリセット（1分ごと）
SELECT reset_stale_idempotency_keys();

-- 3. 期限切れ冪等性キーの削除（1日1回）
DELETE FROM idempotency_keys WHERE expires_at < NOW();

-- 4. 新月のパーティション作成（月末に実行）
CREATE TABLE IF NOT EXISTS events_2026_02 PARTITION OF events
    FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');
CREATE TABLE IF NOT EXISTS ledger_entries_2026_02 PARTITION OF ledger_entries
    FOR VALUES FROM ('2026-02-01') TO ('2026-03-01');
```

---

## 開発ロードマップ

### Phase 1: 基盤構築 ✅
- [x] プロジェクトセットアップ
- [x] 仕様書作成
- [x] イベントソーシング設計
- [x] 1ユーザー1口座モデル設計

### Phase 2: コア実装
- [ ] SQLマイグレーション作成
- [ ] Amount型実装
- [ ] イベントストア実装（append_atomic）
- [ ] Account Aggregate実装
- [ ] スナップショット機能

### Phase 3: API層
- [ ] Axumルーター
- [ ] APIキー認証ミドルウェア
- [ ] Rate Limitingミドルウェア
- [ ] ログマスク化
- [ ] user_id → account_id 変換

### Phase 4: 送金機能
- [ ] TransferCommandHandler実装
- [ ] Projection更新サービス
- [ ] ATP発行（Mint）機能

### Phase 5: 運用
- [ ] バックアップ設定
- [ ] 監査ログ検証ジョブ
- [ ] 定期メンテナンスジョブ

---

## ライセンス

MIT