# ngx_vts: Upstream/Cacheゾーン統計実装方針

## 1. 現状分析

### 既存実装の確認
- 現在のngx_vtsは基本的なserverZones統計のみ実装
- Prometheus形式の出力に対応
- 共有メモリゾーンでの統計管理が実装済み
- ngx-rustフレームワークを使用

### 元のnginx-module-vtsの機能
- **UpstreamZones**: アップストリームグループ内の各サーバーごとの詳細統計
- **CacheZones**: プロキシキャッシュの使用状況とヒット率統計

## 2. Upstreamゾーン統計の実装方針

### 2.1 データ構造の設計

```rust
// src/upstream_stats.rs

#[derive(Debug, Clone)]
pub struct UpstreamServerStats {
    pub server: String,           // サーバーアドレス (例: "10.10.10.11:80")
    pub request_counter: u64,     // リクエスト数
    pub in_bytes: u64,            // 受信バイト数
    pub out_bytes: u64,           // 送信バイト数
    pub responses: ResponseStats, // レスポンス統計（既存のものを再利用）
    pub request_time_total: u64,  // 累計リクエスト処理時間（ミリ秒）
    pub request_time_counter: u64,// リクエスト時間カウンター
    pub response_time_total: u64, // アップストリームレスポンス時間
    pub response_time_counter: u64,
    
    // Nginx設定情報
    pub weight: u32,              // サーバーの重み
    pub max_fails: u32,           // max_fails設定
    pub fail_timeout: u32,        // fail_timeout設定
    pub backup: bool,             // バックアップサーバーフラグ
    pub down: bool,               // ダウン状態フラグ
}

#[derive(Debug, Clone)]
pub struct UpstreamZone {
    pub name: String,                                    // アップストリームグループ名
    pub servers: HashMap<String, UpstreamServerStats>,   // サーバーごとの統計
}
```

### 2.2 統計収集の実装

```rust
// nginxリクエストフェーズでのフック

impl UpstreamStatsCollector {
    pub fn log_upstream_request(&mut self, 
        upstream_name: &str,
        upstream_addr: &str,
        request_time: u64,
        upstream_response_time: u64,
        bytes_sent: u64,
        bytes_received: u64,
        status_code: u16) {
        
        // 共有メモリゾーンから統計を取得・更新
        let zone = self.get_or_create_upstream_zone(upstream_name);
        let server_stats = zone.servers.entry(upstream_addr.to_string())
            .or_insert_with(|| UpstreamServerStats::new(upstream_addr));
        
        // 統計を更新
        server_stats.request_counter += 1;
        server_stats.in_bytes += bytes_received;
        server_stats.out_bytes += bytes_sent;
        server_stats.update_response_status(status_code);
        server_stats.update_timing(request_time, upstream_response_time);
    }
}
```

### 2.3 Nginxインテグレーション

```rust
// nginxのupstream選択後のフックポイント

use ngx_rust::core::*;

pub fn register_upstream_hooks() {
    // log_phaseでのフック登録
    ngx_http_log_handler!(upstream_log_handler);
}

fn upstream_log_handler(request: &Request) -> Status {
    if let Some(upstream_state) = request.upstream_state() {
        // アップストリーム情報の取得
        let upstream_name = upstream_state.upstream_name();
        let upstream_addr = upstream_state.peer_addr();
        let response_time = upstream_state.response_time();
        
        // 統計を記録
        with_shared_zone(|zone| {
            zone.log_upstream_request(
                upstream_name,
                upstream_addr,
                request.request_time(),
                response_time,
                request.bytes_sent(),
                request.bytes_received(),
                request.status()
            );
        });
    }
    
    Status::OK
}
```

### 2.4 Prometheusメトリクス出力

```rust
// Upstream関連のメトリクス追加

impl PrometheusFormatter {
    pub fn format_upstream_stats(&self, zones: &[UpstreamZone]) -> String {
        let mut output = String::new();
        
        // アップストリームリクエスト数
        output.push_str("# HELP nginx_vts_upstream_requests_total Total upstream requests\n");
        output.push_str("# TYPE nginx_vts_upstream_requests_total counter\n");
        
        for zone in zones {
            for (addr, stats) in &zone.servers {
                output.push_str(&format!(
                    "nginx_vts_upstream_requests_total{{upstream=\"{}\",server=\"{}\"}} {}\n",
                    zone.name, addr, stats.request_counter
                ));
            }
        }
        
        // バイト転送量
        output.push_str("# HELP nginx_vts_upstream_bytes_total Total bytes transferred\n");
        output.push_str("# TYPE nginx_vts_upstream_bytes_total counter\n");
        
        for zone in zones {
            for (addr, stats) in &zone.servers {
                output.push_str(&format!(
                    "nginx_vts_upstream_bytes_total{{upstream=\"{}\",server=\"{}\",direction=\"in\"}} {}\n",
                    zone.name, addr, stats.in_bytes
                ));
                output.push_str(&format!(
                    "nginx_vts_upstream_bytes_total{{upstream=\"{}\",server=\"{}\",direction=\"out\"}} {}\n",
                    zone.name, addr, stats.out_bytes
                ));
            }
        }
        
        // レスポンス時間
        output.push_str("# HELP nginx_vts_upstream_response_seconds Upstream response time\n");
        output.push_str("# TYPE nginx_vts_upstream_response_seconds gauge\n");
        
        // サーバー状態
        output.push_str("# HELP nginx_vts_upstream_server_up Upstream server status\n");
        output.push_str("# TYPE nginx_vts_upstream_server_up gauge\n");
        
        output
    }
}
```

## 3. Cacheゾーン統計の実装方針

### 3.1 データ構造の設計

```rust
// src/cache_stats.rs

#[derive(Debug, Clone)]
pub struct CacheZoneStats {
    pub name: String,          // キャッシュゾーン名
    pub max_size: u64,         // 最大サイズ（設定値）
    pub used_size: u64,        // 使用中のサイズ
    pub in_bytes: u64,         // キャッシュから読み込んだバイト数
    pub out_bytes: u64,        // キャッシュに書き込んだバイト数
    
    // キャッシュヒット統計
    pub responses: CacheResponses,
}

#[derive(Debug, Clone, Default)]
pub struct CacheResponses {
    pub miss: u64,             // キャッシュミス
    pub bypass: u64,           // キャッシュバイパス
    pub expired: u64,          // 期限切れ
    pub stale: u64,           // 古いキャッシュ使用
    pub updating: u64,         // 更新中
    pub revalidated: u64,      // 再検証済み
    pub hit: u64,              // キャッシュヒット
    pub scarce: u64,           // メモリ不足
}
```

### 3.2 キャッシュ統計の収集

```rust
impl CacheStatsCollector {
    pub fn log_cache_access(&mut self,
        cache_zone_name: &str,
        cache_status: CacheStatus,
        bytes_transferred: u64) {
        
        let zone_stats = self.get_or_create_cache_zone(cache_zone_name);
        
        // キャッシュステータスに応じて統計を更新
        match cache_status {
            CacheStatus::Hit => {
                zone_stats.responses.hit += 1;
                zone_stats.in_bytes += bytes_transferred;
            },
            CacheStatus::Miss => {
                zone_stats.responses.miss += 1;
                zone_stats.out_bytes += bytes_transferred;
            },
            CacheStatus::Expired => {
                zone_stats.responses.expired += 1;
            },
            CacheStatus::Bypass => {
                zone_stats.responses.bypass += 1;
            },
            CacheStatus::Stale => {
                zone_stats.responses.stale += 1;
            },
            CacheStatus::Updating => {
                zone_stats.responses.updating += 1;
            },
            CacheStatus::Revalidated => {
                zone_stats.responses.revalidated += 1;
            },
        }
    }
    
    pub fn update_cache_size(&mut self, cache_zone_name: &str, used_size: u64) {
        if let Some(zone_stats) = self.cache_zones.get_mut(cache_zone_name) {
            zone_stats.used_size = used_size;
        }
    }
}
```

### 3.3 Nginxキャッシュとの統合

```rust
// nginxのキャッシュ変数から情報を取得

fn cache_log_handler(request: &Request) -> Status {
    // $upstream_cache_status変数から状態を取得
    if let Some(cache_status) = request.var("upstream_cache_status") {
        let cache_zone = request.var("proxy_cache").unwrap_or_default();
        
        let status = match cache_status.as_str() {
            "HIT" => CacheStatus::Hit,
            "MISS" => CacheStatus::Miss,
            "EXPIRED" => CacheStatus::Expired,
            "BYPASS" => CacheStatus::Bypass,
            "STALE" => CacheStatus::Stale,
            "UPDATING" => CacheStatus::Updating,
            "REVALIDATED" => CacheStatus::Revalidated,
            _ => return Status::OK,
        };
        
        with_shared_zone(|zone| {
            zone.log_cache_access(
                &cache_zone,
                status,
                request.bytes_sent()
            );
        });
    }
    
    Status::OK
}
```

### 3.4 Prometheusメトリクス出力

```rust
impl PrometheusFormatter {
    pub fn format_cache_stats(&self, caches: &[CacheZoneStats]) -> String {
        let mut output = String::new();
        
        // キャッシュサイズ
        output.push_str("# HELP nginx_vts_cache_size_bytes Cache size in bytes\n");
        output.push_str("# TYPE nginx_vts_cache_size_bytes gauge\n");
        
        for cache in caches {
            output.push_str(&format!(
                "nginx_vts_cache_size_bytes{{zone=\"{}\",type=\"max\"}} {}\n",
                cache.name, cache.max_size
            ));
            output.push_str(&format!(
                "nginx_vts_cache_size_bytes{{zone=\"{}\",type=\"used\"}} {}\n",
                cache.name, cache.used_size
            ));
        }
        
        // キャッシュヒット率
        output.push_str("# HELP nginx_vts_cache_hits_total Cache hit statistics\n");
        output.push_str("# TYPE nginx_vts_cache_hits_total counter\n");
        
        for cache in caches {
            output.push_str(&format!(
                "nginx_vts_cache_hits_total{{zone=\"{}\",status=\"hit\"}} {}\n",
                cache.name, cache.responses.hit
            ));
            output.push_str(&format!(
                "nginx_vts_cache_hits_total{{zone=\"{}\",status=\"miss\"}} {}\n",
                cache.name, cache.responses.miss
            ));
            // 他のステータスも同様に出力
        }
        
        output
    }
}
```

## 4. 実装ステップ

### Phase 1: 基盤整備（1-2週間）
1. データ構造の定義（upstream_stats.rs, cache_stats.rs）
2. 共有メモリゾーンの拡張
3. 既存のVTSノードシステムとの統合

### Phase 2: Upstream統計実装（2-3週間）
1. Nginxアップストリーム情報の取得方法調査
2. ログフェーズでのフック実装
3. 統計収集ロジックの実装
4. Prometheusメトリクス出力の追加

### Phase 3: Cache統計実装（2-3週間）
1. Nginxキャッシュ変数の調査
2. キャッシュアクセスの検出と記録
3. キャッシュサイズの監視
4. Prometheusメトリクス出力の追加

### Phase 4: テストと最適化（1-2週間）
1. ユニットテストの作成
2. 統合テストの実装
3. パフォーマンステスト
4. メモリ使用量の最適化

## 5. 技術的課題と解決策

### 課題1: Nginxの内部構造へのアクセス
**問題**: ngx-rustからアップストリームやキャッシュの詳細情報へのアクセスが限定的
**解決策**: 
- nginx変数を活用（$upstream_addr, $upstream_response_time等）
- 必要に応じてngx-rustへのコントリビューション

### 課題2: パフォーマンスへの影響
**問題**: 統計収集によるレイテンシ増加の懸念
**解決策**:
- ロックフリーなデータ構造の採用
- 統計更新のバッチ処理
- 非同期処理の活用

### 課題3: メモリ使用量
**問題**: アップストリームサーバー数が多い場合のメモリ消費
**解決策**:
- LRUキャッシュの実装
- 設定可能な統計保持期間
- 動的メモリ割り当て

## 6. 設定例

```nginx
http {
    # VTSゾーンの設定（拡張版）
    vts_zone main 10m;
    vts_upstream_zone 5m;  # アップストリーム統計用
    vts_cache_zone 2m;      # キャッシュ統計用
    
    upstream backend {
        server 10.10.10.11:80 weight=5;
        server 10.10.10.12:80 weight=3;
        server 10.10.10.13:80 backup;
    }
    
    proxy_cache_path /var/cache/nginx 
                     levels=1:2 
                     keys_zone=my_cache:10m 
                     max_size=1g;
    
    server {
        listen 80;
        
        location / {
            proxy_pass http://backend;
            proxy_cache my_cache;
            
            # VTS統計を有効化
            vts_upstream_stats on;
            vts_cache_stats on;
        }
        
        location /status {
            vts_status;
            vts_format prometheus;
        }
    }
}
```

## 7. 期待される出力例

```prometheus
# Upstream統計
nginx_vts_upstream_requests_total{upstream="backend",server="10.10.10.11:80"} 15234
nginx_vts_upstream_requests_total{upstream="backend",server="10.10.10.12:80"} 9123
nginx_vts_upstream_bytes_total{upstream="backend",server="10.10.10.11:80",direction="in"} 5242880
nginx_vts_upstream_response_seconds{upstream="backend",server="10.10.10.11:80",type="avg"} 0.125
nginx_vts_upstream_server_up{upstream="backend",server="10.10.10.11:80"} 1
nginx_vts_upstream_server_up{upstream="backend",server="10.10.10.13:80"} 0

# Cache統計
nginx_vts_cache_size_bytes{zone="my_cache",type="max"} 1073741824
nginx_vts_cache_size_bytes{zone="my_cache",type="used"} 524288000
nginx_vts_cache_hits_total{zone="my_cache",status="hit"} 8500
nginx_vts_cache_hits_total{zone="my_cache",status="miss"} 1500
nginx_vts_cache_hits_total{zone="my_cache",status="expired"} 234
```

## 8. 今後の拡張可能性

- **JSON出力形式のサポート**: Prometheus以外のモニタリングツール対応
- **FilterZones実装**: より詳細なフィルタリング機能
- **Control API**: 統計のリセット/削除機能
- **WebSocketサポート**: リアルタイム統計ストリーミング
- **gRPCメトリクス**: gRPCバックエンドの統計

## 9. 参考実装

既存のnginx-module-vtsのC実装を参考にしながら、Rustの特性を活かした実装を目指す：
- メモリ安全性の保証
- 並行処理の最適化
- エラーハンドリングの改善
- より表現力の高いコード
