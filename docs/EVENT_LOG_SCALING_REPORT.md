# イベント記録増大時の対応レポート

作成日: 2026-03-05

## 結論

financeATP は、イベント記録が増大しても以下の方針で運用する設計です。

1. `events` テーブルを月次パーティションで分割してスケールさせる。
2. Aggregate 復元は 100 イベントごとのスナップショットで高速化する。
3. イベントは不変（UPDATE/DELETE 禁止）として保持し、履歴を消さない。
4. 管理画面/API でのイベント取得は `limit/offset`（上限 1000）でページングする。

## 実装根拠

### 1. 月次パーティション

- `events` は `created_at` で `PARTITION BY RANGE`。
- 初期マイグレーションで `events_2026_01` から `events_2026_12` まで作成。

根拠:
- `migrations/003_event_sourcing.sql:51`
- `migrations/003_event_sourcing.sql:65`
- `migrations/003_event_sourcing.sql:102`

### 2. スナップショット最適化

- スナップショット間隔は 100 イベント（`SNAPSHOT_INTERVAL = 100`）。
- Aggregate ロード時は、スナップショット取得後に `version > snapshot_version` のイベントのみ再生。
- スナップショット保存は UPSERT で、実質「aggregate ごとに最新 1 件」を保持。

根拠:
- `src/aggregate/mod.rs:29`
- `src/aggregate/mod.rs:30`
- `src/event_store/repository.rs:322`
- `src/event_store/repository.rs:395`
- `src/event_store/repository.rs:410`
- `src/event_store/repository.rs:412`
- `migrations/003_event_sourcing.sql:130`
- `migrations/003_event_sourcing.sql:139`

### 3. イベント不変性

- `events` に `no_modify_events` トリガーを設定し、UPDATE/DELETE を禁止。

根拠:
- `migrations/003_event_sourcing.sql:117`

### 4. 管理 API の取得制御

- `GET /admin/events` は `limit` を最大 1000 に制限し、`offset` でページング。

根拠:
- `src/api/routes.rs:691`
- `src/api/routes.rs:692`
- `src/api/routes.rs:703`

## 補足（運用上の注意）

### 1. 次月パーティション自動作成ロジックは存在

- `create_next_month_partitions()` が実装済み。
- 月末 3 日に実行判定するロジックあり。

根拠:
- `src/jobs/mod.rs:103`
- `src/jobs/mod.rs:305`

### 2. ただし現状はスケジューラ未接続

- `main.rs` では `JobScheduler` 起動処理が入っていないため、
  自動パーティション作成は現状のサーバー起動だけでは動かない。

根拠:
- `src/main.rs:63`

## 現状の整理

- 「イベントが膨大になったら削除する」設計ではない。
- 「分割（パーティション） + 読み込み最適化（スナップショット） + 不変保持」で対応する設計。
- 自動運用を完全化するには、ジョブスケジューラ起動の接続が別途必要。
