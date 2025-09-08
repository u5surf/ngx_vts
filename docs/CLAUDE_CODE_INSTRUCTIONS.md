# Claude Code実装指示書 - ngx_vts Upstream/Cache統計機能

## プロジェクト概要
このプロジェクトは、nginx-module-vtsのRust実装であるngx_vtsに、upstreamとcacheゾーンの統計機能を追加します。

## Phase 1: 基盤整備

### タスク1: データ構造の実装
```
docs/IMPLEMENTATION_PLAN.mdのPhase 1を参照して、以下のファイルを作成してください：

1. src/upstream_stats.rs を新規作成
   - UpstreamServerStats構造体を実装
   - UpstreamZone構造体を実装
   - 必要なderiveマクロ（Debug, Clone, Serialize）を追加

2. src/cache_stats.rs を新規作成
   - CacheZoneStats構造体を実装
   - CacheResponses構造体を実装

3. src/lib.rsでモジュールを登録
   - mod upstream_stats;
   - mod cache_stats;
```

### タスク2: 共有メモリゾーンの拡張
```
src/vts_node.rsを拡張して：
1. VtsNodeにupstream_zonesとcache_zonesフィールドを追加
2. 初期化メソッドを更新
3. アクセサメソッドを実装
```

## Phase 2: Upstream統計実装

### タスク3: Nginxフック実装
```
src/upstream_stats.rsに以下を実装：

1. UpstreamStatsCollector構造体を作成
2. log_upstream_requestメソッドを実装
3. nginxのlog_phaseフックを登録する関数を作成

nginxの以下の変数から情報を取得：
- $upstream_addr
- $upstream_response_time  
- $upstream_status
- $request_time
- $bytes_sent
- $bytes_received
```

### タスク4: Prometheusフォーマッター拡張
```
src/lib.rsまたは新規ファイルsrc/prometheus.rsで：

1. format_upstream_statsメソッドを追加
2. 以下のメトリクスを出力：
   - nginx_vts_upstream_requests_total
   - nginx_vts_upstream_bytes_total
   - nginx_vts_upstream_response_seconds
   - nginx_vts_upstream_server_up
```

## Phase 3: Cache統計実装

### タスク5: キャッシュ統計収集
```
src/cache_stats.rsに以下を実装：

1. CacheStatsCollector構造体を作成
2. log_cache_accessメソッドを実装
3. $upstream_cache_status変数からキャッシュ状態を取得
4. キャッシュゾーン名は$proxy_cache変数から取得
```

### タスク6: キャッシュメトリクス出力
```
Prometheusフォーマッターに追加：
1. format_cache_statsメソッドを実装
2. 以下のメトリクスを出力：
   - nginx_vts_cache_size_bytes
   - nginx_vts_cache_hits_total
```

## Phase 4: 統合とテスト

### タスク7: 設定ディレクティブ追加
```
src/config.rsを更新：
1. vts_upstream_stats on/offディレクティブを追加
2. vts_cache_stats on/offディレクティブを追加
3. パース処理を実装
```

### タスク8: テスト作成
```
tests/ディレクトリに以下のテストを作成：
1. upstream_stats_test.rs - Upstream統計のユニットテスト
2. cache_stats_test.rs - Cache統計のユニットテスト
3. integration_test.rs - 統合テスト
```

## 実装時の注意事項

1. **ngx-rust APIの制限**
   - 利用可能なAPIを確認: https://github.com/nginxinc/ngx-rust
   - 不足している場合は回避策を検討

2. **メモリ安全性**
   - Rustの所有権ルールに従う
   - unsafe使用は最小限に

3. **パフォーマンス**
   - ロック競合を避ける
   - 統計更新は可能な限り非同期で

4. **エラーハンドリング**
   - Result型を適切に使用
   - パニックを避ける

## デバッグとテスト

### ローカルテスト環境セットアップ
```bash
# Nginxテスト設定
cat > test/nginx.conf << 'EOF'
load_module /path/to/libngx_vts_rust.so;

http {
    vts_zone main 10m;
    
    upstream backend {
        server 127.0.0.1:8001;
        server 127.0.0.1:8002;
    }
    
    proxy_cache_path /tmp/nginx_cache levels=1:2 keys_zone=test_cache:10m;
    
    server {
        listen 8080;
        
        location / {
            proxy_pass http://backend;
            proxy_cache test_cache;
        }
        
        location /status {
            vts_status;
        }
    }
}
EOF

# バックエンドサーバー起動（Python）
python3 -m http.server 8001 &
python3 -m http.server 8002 &

# Nginx起動
nginx -c test/nginx.conf
```

### 動作確認
```bash
# リクエスト送信
for i in {1..100}; do
    curl http://localhost:8080/
done

# 統計確認
curl http://localhost:8080/status
```

## コミット規約

各フェーズごとにコミット：
```
feat(upstream): Add upstream statistics data structures
feat(upstream): Implement nginx log phase hook
feat(upstream): Add Prometheus metrics for upstream
feat(cache): Add cache statistics structures
feat(cache): Implement cache access logging
feat(cache): Add Prometheus metrics for cache
test: Add unit tests for upstream statistics
test: Add integration tests
docs: Update README with new features
```

## 質問用テンプレート

実装中に不明な点があれば、以下の形式で質問：

```
【状況】
現在実装中の機能: [upstream統計/cache統計]
ファイル: [対象ファイル名]
行番号: [該当行]

【問題】
[具体的な問題の説明]

【試したこと】
1. [試行1]
2. [試行2]

【エラーメッセージ】
```rust
[エラーメッセージ]
```

【関連コード】
```rust
[関連するコード部分]
```
```

## 段階的な実装アプローチ

最初は最小限の実装から始めることを推奨：

### Step 1: 最小限のUpstream統計
1. 1つのupstreamグループのみ対応
2. request_counterとbytesのみ収集
3. Prometheusで出力確認

### Step 2: 機能拡張
1. 複数のupstreamグループ対応
2. レスポンスタイム統計追加
3. サーバー状態（up/down）追加

### Step 3: Cache統計追加
1. 基本的なhit/miss統計
2. キャッシュサイズ監視
3. 詳細なキャッシュステータス
