# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Shared zone accounting, ported from nginx-module-vts t/033.shm_free_size.t.
#
# That file is not about the JSON display despite its neighbours: its second
# block asserts a Prometheus series. Its header says why the metric has to
# exist at all - the sum of the node sizes is not what the zone has spent,
# because the slab hands out a whole page or slot per node, so a zone can
# refuse an insert while the used figure still reads well below the maximum.
# What an operator needs is what the slab has left.
#
# This port drops used_size for the same reason and reports free_size and a
# node count instead.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 18;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: the zone reports the size it was configured with
--- http_config
    vts_zone main 1m;
--- config
    location /status { vts_status; }
--- request
GET /status
--- response_body_like eval
qr/nginx_vts_main_shm_usage_bytes\{shared="max_size"\} 1048576/



=== TEST 2: a different size is reported as configured
--- http_config
    vts_zone main 2m;
--- config
    location /status { vts_status; }
--- request
GET /status
--- response_body_like eval
qr/nginx_vts_main_shm_usage_bytes\{shared="max_size"\} 2097152/



=== TEST 3: the slab says how much of it is left
--- http_config
    vts_zone main 1m;
--- config
    location /status { vts_status; }
--- request
GET /status
--- response_body_like eval
qr/nginx_vts_main_shm_usage_bytes\{shared="free_size"\} [1-9]\d*/



=== TEST 4: what is left is less than the whole zone
--- http_config
    vts_zone main 1m;
--- config
    location /hello  { return 200 "hello"; }
    location /status { vts_status; }
--- request eval
['GET /hello', 'GET /status']
--- response_body_like eval
[
    qr/\Ahello\z/,
    qr/nginx_vts_main_shm_usage_bytes\{shared="free_size"\} (?!1048576)\d+/,
]



=== TEST 5: nodes are counted as zones appear
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "peer"; }
    }
    upstream backend { server 127.0.0.1:1985; }
--- config
    location /hello  { return 200 "hello"; }
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /hello', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Ahello\z/,
    qr/\Apeer\z/,
    qr/nginx_vts_main_shm_usage_nodes [2-9]\d*/,
]



=== TEST 6: both families are declared gauges
--- http_config
    vts_zone main 1m;
--- config
    location /status { vts_status; }
--- request
GET /status
--- response_body_like eval
qr/# TYPE nginx_vts_main_shm_usage_bytes gauge(.|\n)*# TYPE nginx_vts_main_shm_usage_nodes gauge/
