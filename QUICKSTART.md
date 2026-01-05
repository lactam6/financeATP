# financeATP クイックスタート

## 必要なもの
- [Docker Desktop](https://www.docker.com/products/docker-desktop/)

---

## セットアップ (3ステップ)

### 1. Docker Desktop を起動
タスクバー/メニューバーにDockerアイコンが表示されるまで待つ

### 2. Dockerイメージをインポート
```bash
docker load -i finance_atp.tar
```

### 3. 起動
**Windows:** `start.bat` をダブルクリック

**Mac/Linux:**
```bash
chmod +x start.sh
./start.sh
```

---

## 動作確認
ブラウザで http://localhost:3000/health にアクセス → `OK` と表示されれば成功

## 停止
```bash
docker-compose down
```

## データ初期化（全データ削除）
```bash
docker-compose down -v
docker-compose up -d
```

---

## 開発用APIキー
```
test1234567890abcdef
```

## トラブルシューティング

| 問題                            | 解決策                                     |
| ------------------------------- | ------------------------------------------ |
| Docker Desktop が起動していない | Docker Desktop を起動してからやり直す      |
| ポート 3000 が使用中            | 他のアプリを停止する                       |
| イメージがない                  | `docker load -i finance_atp.tar` を実行    |
| 起動しない                      | `docker logs finance_atp_app` でログを確認 |
