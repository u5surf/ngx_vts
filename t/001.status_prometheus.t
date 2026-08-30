# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Ported from nginx-module-vts t/022.display_prometheus.t.
#
# The original serves several formats and switches between them with
# vhost_traffic_status_display_format; this module only speaks Prometheus, so
# what is left to check is that /status is that, and that the exposition is
# well formed.

use Test::Nginx::Socket;

plan tests => repeat_each(2) * blocks() * 3;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: the status handler serves Prometheus text
--- http_config
    vts_zone main 1m;
--- config
    location /status {
        vts_status;
    }
--- request
GET /status
--- response_headers_like
Content-Type: text/plain.*
--- response_body_like eval
qr/nginx_vts_info\{hostname="[^"]+",version="[^"]+"\} 1/



=== TEST 2: every metric family carries HELP and TYPE
--- http_config
    vts_zone main 1m;
--- config
    location /status {
        vts_status;
    }
--- request
GET /status
--- response_headers_like
Content-Type: text/plain.*
--- response_body_like eval
qr/# HELP nginx_vts_info .+\n# TYPE nginx_vts_info gauge/



=== TEST 3: connection metrics are present without any traffic
--- http_config
    vts_zone main 1m;
--- config
    location /status {
        vts_status;
    }
--- request
GET /status
--- response_headers_like
Content-Type: text/plain.*
--- response_body_like eval
qr/nginx_vts_connections\{state="active"\} \d+/



=== TEST 4: the zone directive is what turns collection on
--- http_config
    vts_zone main 1m;
--- config
    location /status {
        vts_status;
    }
--- request
GET /status
--- response_headers_like
Content-Type: text/plain.*
--- response_body_like eval
qr/nginx_vts_connections_total\{state="accepted"\} \d+/
