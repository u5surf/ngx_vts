# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# process_start_time_seconds, which nginx-module-vts does not have.
#
# The counters reset whenever the zone is rebuilt, and a scraper that only
# sees the counters cannot tell that reset from a series it has just started
# watching. The OpenTelemetry Collector's metricstarttime processor and
# Datadog's use_process_start_time both look for this metric to tell them
# apart, so it carries the time the zone was built - the time the counters
# were last zero - under the name they look for.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 8;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: the start time is reported when a zone is configured
--- http_config
    vts_zone main 1m;
--- config
    location /status { vts_status; }
--- request
GET /status
--- response_body_like eval
qr/\nprocess_start_time_seconds \d{10}\n/



=== TEST 2: it is not in the future and not long past
--- http_config
    vts_zone main 1m;
--- config
    location /status { vts_status; }
--- request
GET /status
--- response_body_like eval
my $now = time;
qr/\nprocess_start_time_seconds (\d+)\n(?(?{ $1 <= $now && $1 > $now - 300 })|(*FAIL))/



=== TEST 3: it is unprefixed and declared a gauge
--- http_config
    vts_zone main 1m;
--- config
    location /status { vts_status; }
--- request
GET /status
--- response_body_like eval
qr/# TYPE process_start_time_seconds gauge\n/



=== TEST 4: without a zone there is nothing to report
--- config
    location /status { vts_status; }
--- request
GET /status
--- response_body_unlike eval
qr/process_start_time_seconds/
