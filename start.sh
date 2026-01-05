#!/bin/bash
# financeATP - ワンクリック起動スクリプト (Mac/Linux)
# 使用方法: ./start.sh

set -e

echo ""
echo "============================================"
echo "  financeATP 起動中..."
echo "============================================"
echo ""

# Docker が起動しているか確認
if ! docker info > /dev/null 2>&1; then
    echo "[エラー] Docker が起動していません。"
    echo "Docker Desktop を起動してから再実行してください。"
    exit 1
fi

# コンテナを起動
echo "[1/3] コンテナを起動しています..."
docker-compose up -d

# 起動待機
echo "[2/3] サービスの起動を待機しています..."
sleep 10

# ヘルスチェック
echo "[3/3] 接続を確認しています..."
until curl -s http://localhost:3000/health > /dev/null 2>&1; do
    echo "  まだ起動中... (数秒お待ちください)"
    sleep 3
done

echo ""
echo "============================================"
echo "  起動完了！"
echo "============================================"
echo ""
echo "  API: http://localhost:3000"
echo ""
echo "  停止するには: docker-compose down"
echo ""

# ブラウザで開く（オプション）
# open http://localhost:3000/health  # Mac
# xdg-open http://localhost:3000/health  # Linux
