# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Upstream response time histogram, ported from nginx-module-vts
# t/023.histogram_buckets.t.
#
# The original configures its own bucket boundaries with
# vhost_traffic_status_histogram_buckets and checks they come out in the
# display. This module has no such directive: it emits a fixed layout matching
# client_golang's defaults, so what is checked here is that the layout is a
# well formed Prometheus histogram - the +Inf bucket, _sum and _count, and
# buckets that never decrease as le grows.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 16;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: a histogram is emitted for a peer that served a request
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "peer"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Apeer\z/,
    qr/nginx_vts_upstream_response_duration_seconds_bucket\{upstream="backend",server="127\.0\.0\.1:1985",le="[0-9.]+"\} \d+/,
]



=== TEST 2: the histogram is closed by an +Inf bucket
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "peer"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Apeer\z/,
    qr/nginx_vts_upstream_response_duration_seconds_bucket\{[^}]*le="\+Inf"\} [1-9]\d*/,
]



=== TEST 3: _sum and _count accompany the buckets
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "peer"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Apeer\z/,
    qr/nginx_vts_upstream_response_duration_seconds_sum\{[^}]*\} [0-9.]+\n.*nginx_vts_upstream_response_duration_seconds_count\{[^}]*\} [1-9]\d*/,
]



=== TEST 4: the family is declared a histogram
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "peer"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Apeer\z/,
    qr/# TYPE nginx_vts_upstream_response_duration_seconds histogram/,
]
