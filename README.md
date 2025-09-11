# nginx-vts-rust

[![CI](https://github.com/u5surf/ngx_vts/actions/workflows/ci.yml/badge.svg)](https://github.com/u5surf/ngx_vts/actions/workflows/ci.yml)

A Rust implementation of nginx-module-vts for virtual host traffic status monitoring, built using the ngx-rust framework.

**This module is EXPERIMENTAL.**

## Features

- **Real-time Traffic Monitoring**: Comprehensive statistics collection for Nginx virtual hosts
- **Upstream Statistics**: Complete upstream server monitoring with per-server metrics
- **Prometheus Metrics**: Native Prometheus format output for monitoring integration
- **Zone-based Statistics**: Per-server zone traffic tracking
- **Request Metrics**: Detailed request/response statistics including timing and status codes
- **Connection Tracking**: Active connection monitoring
- **Shared Memory**: Efficient statistics storage using nginx shared memory zones
- **Thread-safe**: Concurrent statistics collection and retrieval
- **Load Balancer Monitoring**: Track upstream server health, response times, and status codes

## Building

### Prerequisites

- Rust 1.81 or later
- Nginx source code (required for compilation)
- ngx-rust framework
- GCC/Clang compiler for C wrapper components

### Build Steps

1. Clone this repository:
```bash
git clone <repository-url>
cd ngx_vts_rust
```

2. Download and extract nginx source:
```bash
wget http://nginx.org/download/nginx-1.28.0.tar.gz
tar -xzf nginx-1.28.0.tar.gz
```

3. Set environment variable and build Rust library:
```bash
export NGINX_SOURCE_DIR=/path/to/nginx-1.28.0
cargo build --release
```

4. Configure and build nginx with VTS module:
```bash
cd nginx-1.28.0
./configure --with-compat --add-dynamic-module=/path/to/ngx_vts
make
```

The compiled dynamic module will be available at `nginx-1.28.0/objs/ngx_http_vts_module.so`.

## Configuration

### Nginx Configuration

Add the following to your nginx configuration:

```nginx
# Load the VTS module
load_module /path/to/ngx_http_vts_module.so;

http {
    # Configure shared memory zone for VTS statistics
    vts_zone main 10m;
    
    # Enable upstream statistics collection (optional)
    vts_upstream_stats on;
    
    # Define upstream groups for load balancing
    upstream backend {
        server 10.0.0.1:8080;
        server 10.0.0.2:8080;
        server 10.0.0.3:8080 backup;
    }
    
    upstream api_backend {
        server 192.168.1.10:9090;
        server 192.168.1.11:9090;
    }
    
    server {
        listen 80;
        server_name example.com;
        
        # Proxy to upstream with statistics tracking
        location /api/ {
            proxy_pass http://api_backend;
            proxy_set_header Host $host;
        }
        
        location / {
            proxy_pass http://backend;
            proxy_set_header Host $host;
        }
        
        # VTS status endpoint
        location /status {
            vts_status;
            allow 127.0.0.1;
            deny all;
        }
    }
}
```

### Available Directives

- **`vts_status`**: Enable the VTS status endpoint for this location
- **`vts_zone <zone_name> <size>`**: Configure a shared memory zone for VTS statistics storage
  - `zone_name`: Name of the shared memory zone (e.g., "main")  
  - `size`: Size of the shared memory zone (e.g., "1m", "10m")
  - Example: `vts_zone main 10m`
- **`vts_upstream_stats on|off`**: Enable or disable upstream server statistics collection
  - Default: `off`
  - When enabled, tracks detailed statistics for all upstream servers
  - Includes request counts, response times, byte transfers, and status codes

## Usage

Once configured, access the status endpoint:

```bash
curl http://localhost/status
```

### Prometheus Metrics Output

The module outputs metrics in Prometheus format:

```
# HELP nginx_vts_info Nginx VTS module information
# TYPE nginx_vts_info gauge
nginx_vts_info{hostname="example.com",version="0.1.0"} 1

# HELP nginx_vts_connections Current nginx connections
# TYPE nginx_vts_connections gauge
nginx_vts_connections{state="active"} 1
nginx_vts_connections{state="reading"} 0
nginx_vts_connections{state="writing"} 1
nginx_vts_connections{state="waiting"} 0

# HELP nginx_vts_connections_total Total nginx connections
# TYPE nginx_vts_connections_total counter
nginx_vts_connections_total{state="accepted"} 16
nginx_vts_connections_total{state="handled"} 16

# HELP nginx_vts_server_requests_total Total number of requests
# TYPE nginx_vts_server_requests_total counter
nginx_vts_server_requests_total{zone="example.com"} 1000

# HELP nginx_vts_server_bytes_total Total bytes transferred
# TYPE nginx_vts_server_bytes_total counter
nginx_vts_server_bytes_total{zone="example.com",direction="in"} 50000
nginx_vts_server_bytes_total{zone="example.com",direction="out"} 2000000

# HELP nginx_vts_server_responses_total Total responses by status code
# TYPE nginx_vts_server_responses_total counter
nginx_vts_server_responses_total{zone="example.com",status="2xx"} 950
nginx_vts_server_responses_total{zone="example.com",status="4xx"} 15
nginx_vts_server_responses_total{zone="example.com",status="5xx"} 5

# HELP nginx_vts_server_request_seconds Request processing time
# TYPE nginx_vts_server_request_seconds gauge
nginx_vts_server_request_seconds{zone="example.com",type="avg"} 0.125
nginx_vts_server_request_seconds{zone="example.com",type="min"} 0.001
nginx_vts_server_request_seconds{zone="example.com",type="max"} 2.5

# HELP nginx_vts_upstream_requests_total Total upstream requests
# TYPE nginx_vts_upstream_requests_total counter
nginx_vts_upstream_requests_total{upstream="backend",server="10.0.0.1:8080"} 500
nginx_vts_upstream_requests_total{upstream="backend",server="10.0.0.2:8080"} 450
nginx_vts_upstream_requests_total{upstream="api_backend",server="192.168.1.10:9090"} 200

# HELP nginx_vts_upstream_bytes_total Total bytes transferred to/from upstream
# TYPE nginx_vts_upstream_bytes_total counter
nginx_vts_upstream_bytes_total{upstream="backend",server="10.0.0.1:8080",direction="in"} 250000
nginx_vts_upstream_bytes_total{upstream="backend",server="10.0.0.1:8080",direction="out"} 750000

# HELP nginx_vts_upstream_response_seconds Upstream response time statistics
# TYPE nginx_vts_upstream_response_seconds gauge
nginx_vts_upstream_response_seconds{upstream="backend",server="10.0.0.1:8080",type="request_avg"} 0.050000
nginx_vts_upstream_response_seconds{upstream="backend",server="10.0.0.1:8080",type="upstream_avg"} 0.025000

# HELP nginx_vts_upstream_server_up Upstream server status (1=up, 0=down)
# TYPE nginx_vts_upstream_server_up gauge
nginx_vts_upstream_server_up{upstream="backend",server="10.0.0.1:8080"} 1
nginx_vts_upstream_server_up{upstream="backend",server="10.0.0.2:8080"} 1

# HELP nginx_vts_upstream_responses_total Upstream responses by status code
# TYPE nginx_vts_upstream_responses_total counter
nginx_vts_upstream_responses_total{upstream="backend",server="10.0.0.1:8080",status="2xx"} 480
nginx_vts_upstream_responses_total{upstream="backend",server="10.0.0.1:8080",status="4xx"} 15
nginx_vts_upstream_responses_total{upstream="backend",server="10.0.0.1:8080",status="5xx"} 5
```

## Architecture

The module consists of several key components:

### Core Components
- **VTS Node System** (`src/vts_node.rs`): Core statistics data structures and management
- **Upstream Statistics** (`src/upstream_stats.rs`): Upstream server monitoring and statistics collection
- **Prometheus Formatter** (`src/prometheus.rs`): Metrics output in Prometheus format
- **Configuration** (`src/config.rs`): Module configuration and directives  
- **Main Module** (`src/lib.rs`): Nginx module integration and request handlers

### Nginx Integration Layer
- **C Module Wrapper** (`src/ngx_http_vts_module.c`): Main nginx module definition and directive handlers
- **LOG_PHASE Handler** (`src/ngx_vts_wrapper.c`): Real-time upstream request tracking via nginx LOG_PHASE
- **FFI Bridge**: Seamless integration between C nginx module and Rust implementation

### Real-time Statistics Collection

The module implements a **LOG_PHASE handler** that captures every upstream request in real-time:

1. **Request Interception**: Each upstream request triggers the LOG_PHASE handler
2. **Data Extraction**: Handler extracts upstream name, server address, timing, bytes, and status code
3. **Rust Integration**: Extracted data is passed to Rust via `vts_track_upstream_request()` FFI function
4. **Statistics Update**: Rust components update shared statistics immediately
5. **Live Metrics**: Statistics are available instantly via the `/status` endpoint

### Shared Memory Configuration

The `vts_zone` directive configures a shared memory zone that stores VTS statistics:

- **Zone Name**: Identifies the shared memory zone (typically "main")
- **Zone Size**: Allocates memory for statistics storage (e.g., "1m" = 1MB, "10m" = 10MB)
- **Multi-worker Support**: Statistics are shared across all nginx worker processes
- **Persistence**: Statistics persist across configuration reloads

### Request Tracking

Every request is tracked with the following metrics:
- Request count and timing
- Bytes transferred (in/out)  
- HTTP status code distribution (1xx, 2xx, 3xx, 4xx, 5xx)
- Server zone identification
- Request time statistics (total, max, average)

### Upstream Server Monitoring

When `vts_upstream_stats` is enabled, the module tracks:
- **Per-server metrics**: Individual statistics for each upstream server
- **Request routing**: Which upstream server handled each request
- **Response times**: Both total request time and upstream-specific response time
- **Server health**: Track which servers are up or down
- **Load balancing efficiency**: Monitor request distribution across servers
- **Error rates**: Track 4xx/5xx responses per upstream server

## Monitoring Integration

The Prometheus metrics output integrates seamlessly with monitoring systems:

- **Prometheus**: Direct scraping of metrics endpoint
- **Grafana**: Use Prometheus data source for visualization and upstream server dashboards
- **Alertmanager**: Set up alerts based on metrics thresholds (e.g., upstream server down, high error rates)
- **Load Balancer Monitoring**: Track upstream server health and performance in real-time

### Example Grafana Queries

```promql
# Upstream server request rate
rate(nginx_vts_upstream_requests_total[5m])

# Upstream server error rate
rate(nginx_vts_upstream_responses_total{status=~"4xx|5xx"}[5m])

# Average upstream response time
nginx_vts_upstream_response_seconds{type="upstream_avg"}

# Upstream servers that are down
nginx_vts_upstream_server_up == 0
```

## Development

### Testing

```bash
# Run all tests (including integration tests)
NGINX_SOURCE_DIR=/path/to/nginx-source cargo test

# Run specific test modules
cargo test upstream_stats
cargo test prometheus
cargo test vts_node

# Build with debug information
NGX_DEBUG=1 cargo build
```

The test suite includes:
- Unit tests for all core components
- Integration tests for the complete upstream monitoring pipeline
- Thread-safety tests for concurrent access
- Performance tests with large datasets
- Prometheus metrics format validation

### Contributing

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the Apache License 2.0 - see the LICENSE file for details.

## Comparison with Original nginx-module-vts

This Rust implementation provides:
- ✅ Core VTS functionality
- ✅ Real-time upstream server statistics and monitoring
- ✅ Prometheus metrics output
- ✅ Zone-based statistics with live updates
- ✅ Request/response tracking via LOG_PHASE handlers
- ✅ Load balancer health monitoring
- ✅ Thread-safe concurrent access
- ❌ JSON output (Prometheus only)
- ❌ HTML dashboard (Prometheus only)
- ❌ Control features (reset/delete zones)
- ❌ Cache statistics (removed in favor of upstream focus)
- ❌ Advanced filtering (planned for future versions)

## Performance

The Rust implementation leverages:
- Zero-copy string handling where possible
- Efficient shared memory usage
- Minimal request processing overhead via LOG_PHASE handlers
- Real-time statistics updates without caching delays
- Thread-safe concurrent access

Benchmarks show comparable performance to the original C implementation with improved memory safety and real-time capabilities.