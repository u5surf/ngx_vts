# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Cache-zone counters, ported from the cache half of nginx-module-vts
# t/002.check_json_syntax.t. That block drives server zones, filter zones,
# upstreams and two proxy_cache_path zones through one sequence of twelve
# requests and validates the display after each; the cache part is the piece
# with an equivalent here.
#
# The counters are keyed on the keys_zone name from proxy_cache_path.

use Test::Nginx::Socket;

# The cache directories are named after this process. A fixed path survives
# between runs, and the first request of a later run then finds a stale entry
# and is counted as EXPIRED rather than MISS. Putting them under the server
# root instead is worse: nginx creates subdirectories there that the harness
# cannot clean up, and every later run refuses to start.
our $run = $$;

repeat_each(1);
plan tests => 24;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: the first request through a cache is a miss
--- http_config eval
"    vts_zone main 1m;
    proxy_cache_path /tmp/vts_t005_$::run\_a levels=1:2 keys_zone=cache_one:2m inactive=1m max_size=4m;

    server {
        listen 1985;
        location / { return 200 \"origin\"; }
    }
    upstream backend { server 127.0.0.1:1985; }"
--- config
    location /c {
        proxy_cache cache_one;
        proxy_cache_valid 200 10s;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /c', 'GET /status']
--- response_body_like eval
[
    qr/origin/,
    qr/nginx_vts_cache_requests_total\{zone="cache_one",status="miss"\} [1-9]\d*/,
]



=== TEST 2: a repeated request is a hit
--- http_config eval
"    vts_zone main 1m;
    proxy_cache_path /tmp/vts_t005_$::run\_b levels=1:2 keys_zone=cache_two:2m inactive=1m max_size=4m;

    server {
        listen 1985;
        location / { return 200 \"origin\"; }
    }
    upstream backend { server 127.0.0.1:1985; }"
--- config
    location /c {
        proxy_cache cache_two;
        proxy_cache_valid 200 10s;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /c', 'GET /c', 'GET /status']
--- response_body_like eval
[
    qr/origin/,
    qr/origin/,
    qr/nginx_vts_cache_requests_total\{zone="cache_two",status="hit"\} [1-9]\d*/,
]



=== TEST 3: every cache status has a series, counted or not
--- http_config eval
"    vts_zone main 1m;
    proxy_cache_path /tmp/vts_t005_$::run\_c levels=1:2 keys_zone=cache_three:2m inactive=1m max_size=4m;

    server {
        listen 1985;
        location / { return 200 \"origin\"; }
    }
    upstream backend { server 127.0.0.1:1985; }"
--- config
    location /c {
        proxy_cache cache_three;
        proxy_cache_valid 200 10s;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /c', 'GET /status']
--- response_body_like eval
[
    qr/origin/,
    qr/status="bypass".*\n.*status="expired".*\n.*status="stale".*\n.*status="updating".*\n.*status="revalidated".*\n.*status="scarce"/,
]



=== TEST 4: the zone reports its configured maximum and current usage
--- http_config eval
"    vts_zone main 1m;
    proxy_cache_path /tmp/vts_t005_$::run\_d levels=1:2 keys_zone=cache_four:2m inactive=1m max_size=4m;

    server {
        listen 1985;
        location / { return 200 \"origin\"; }
    }
    upstream backend { server 127.0.0.1:1985; }"
--- config
    location /c {
        proxy_cache cache_four;
        proxy_cache_valid 200 10s;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /c', 'GET /status']
--- response_body_like eval
[
    qr/origin/,
    qr/nginx_vts_cache_size_bytes\{zone="cache_four",type="max"\} [1-9]\d*/,
]



=== TEST 5: two cache zones are counted apart
--- http_config eval
"    vts_zone main 1m;
    proxy_cache_path /tmp/vts_t005_$::run\_e levels=1:2 keys_zone=cache_five:2m inactive=1m max_size=4m;
    proxy_cache_path /tmp/vts_t005_$::run\_f levels=1:2 keys_zone=cache_six:2m inactive=1m max_size=4m;

    server {
        listen 1985;
        location / { return 200 \"origin\"; }
    }
    upstream backend { server 127.0.0.1:1985; }"
--- config
    location /five {
        proxy_cache cache_five;
        proxy_cache_valid 200 10s;
        proxy_pass http://backend/;
    }
    location /six {
        proxy_cache cache_six;
        proxy_cache_valid 200 10s;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /five', 'GET /six', 'GET /status']
--- response_body_like eval
[
    qr/origin/,
    qr/origin/,
    qr/zone="cache_five".*\n(.*\n)*.*zone="cache_six"/,
]
