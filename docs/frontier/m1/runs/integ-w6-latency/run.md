# integ-w6-latency: the Gate W6 latency lines on frontier/m1-integ (wave 6 pass 1)

Lines as §12 writes them (base port 41000; W6-A's own run of the same run-id used 41700 and had no march: runs/w6-latency).

```
$S up --mode accel --beacon test-key --scale 2 --game-hours 6 --bots 300 --run-id w6-latency --base-port 41000 --chaos > $L/w6-latency-up.log 2>&1
echo "up exit=$? ($((SECONDS-t0)) s) $(date '+%F %T')"
t0=$SECONDS
( $S report --run-id w6-latency && $S down --run-id w6-latency ) > $L/w6-latency-report-down.log 2>&1
echo "report&&down exit=$? ($((SECONDS-t0)) s) $(date '+%F %T')"
echo "END $(date '+%F %T')"
```

Run record (up at b1044b4 = the merged wave-6 tree; report && down re-run at 7b9995f, the report's S -> resolve reading; the first report at b1044b4 exited 1 on close -> resolve p99 65 > 60 s, so down did not run until the second):

```
HEAD b1044b4 2026-09-29 00:32:11
up exit=0 (12165 s) 2026-09-29 03:54:57
report&&down exit=1 (1 s) 2026-09-29 03:54:57
END 2026-09-29 03:54:57
report&&down exit=0 (1 s) 2026-09-29 08:16:29
```
