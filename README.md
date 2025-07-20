# nginx-vts-rust

A Rust implementation of nginx-module-vts for virtual host traffic status monitoring, built using the ngx-rust framework.

**This module is EXPERIMENTAL.**

## Features

- **Real-time Traffic Monitoring**: Comprehensive statistics collection for Nginx virtual hosts
- **Prometheus Metrics**: Native Prometheus format output for monitoring integration
- **Zone-based Statistics**: Per-server zone traffic tracking
- **Request Metrics**: Detailed request/response statistics including timing and status codes
- **Connection Tracking**: Active connection monitoring
- **Shared Memory**: Efficient statistics storage using nginx shared memory zones
- **Thread-safe**: Concurrent statistics collection and retrieval

## Building

### Prerequisites

- Rust 1.70 or later
- Nginx source code or development headers
- ngx-rust framework

### Build Steps

1. Clone this repository:
```bash
git clone <repository-url>
cd ngx_vts_rust
```

2. Set environment variables:
```bash
export NGX_VERSION=1.24.0  # Your nginx version
export NGX_DEBUG=1         # Optional: enable debug mode
```

3. Build the module:
```bash
cargo build --release
```

The compiled module will be available at `target/release/libngx_vts_rust.so`.

## Configuration

### Nginx Configuration

Add the following to your nginx configuration:

```nginx
# Load the module
load_module /path/to/libngx_vts_rust.so;

http {
    # Enable VTS zone tracking globally
    vts_status_zone on;
    
    server {
        listen 80;
        server_name example.com;
        
        # Your regular server configuration
        location / {
            # Regular content
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
- **`vts_status_zone on|off`**: Enable/disable zone tracking (default: on)

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
```

## Architecture

The module consists of several key components:

- **Statistics Collection** (`src/stats.rs`): Core data structures and management
- **HTTP Handlers** (`src/handlers.rs`): Request processing and JSON output
- **Configuration** (`src/config.rs`): Module configuration and directives
- **Main Module** (`src/lib.rs`): Nginx module integration and request hooks

### Statistics Collection

The module uses nginx's log phase to collect request statistics without impacting request processing performance. Statistics are stored in shared memory for efficient access across worker processes.

### Request Tracking

Every request is tracked with the following metrics:
- Request count and timing
- Bytes transferred (in/out)
- HTTP status code distribution
- Server zone identification

## Monitoring Integration

The Prometheus metrics output integrates seamlessly with monitoring systems:

- **Prometheus**: Direct scraping of metrics endpoint
- **Grafana**: Use Prometheus data source for visualization
- **Alertmanager**: Set up alerts based on metrics thresholds

## Development

### Testing

```bash
# Run tests
cargo test

# Build with debug information
NGX_DEBUG=1 cargo build
```

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
- ✅ Prometheus metrics output
- ✅ Zone-based statistics
- ✅ Request/response tracking
- ❌ JSON output (Prometheus only)
- ❌ HTML dashboard (Prometheus only)
- ❌ Control features (reset/delete zones)
- ❌ Advanced filtering (planned for future versions)

## Performance

The Rust implementation leverages:
- Zero-copy string handling where possible
- Efficient shared memory usage
- Minimal request processing overhead
- Thread-safe concurrent access

Benchmarks show comparable performance to the original C implementation with improved memory safety.