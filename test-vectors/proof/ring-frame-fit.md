Measured with the frame encoders, SPP's account builders and the v1 compiler. Ceilings 4096 B, 64 accounts, 12 signatures.
min: tracking root, no co-signer, no leg, no fee, the payer an owner where it may be. full: own, curator and tracking roots, one co-signer, 4 SOL legs, a fee, an outside payer, a body on every slot and one exit. The owner run is the ceilings' bound under full, the key's rows bound it apart. A shape's every input and output slot is published.
Every row is one transaction. fits: the row clears the transaction and CPI ceilings. limit: what refuses a row, rail for a shape the rail has no SPP key for, else the excess in accounts, signatures, bytes or CPI data at that row's load. A missing catalog key has no encoded measurement.
| shape | rail | load | catalog key | bytes | accounts | signatures | owner run | frame | SPP CPI | fits | limit |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1x2 | confidential signer | min | yes | 1259 | 9 | 1 | - | 843 | 857 | yes |  |
| 1x2 | confidential signer | full | yes | 1918 | 19 | 3 | 1 | 1034 | 1006 | yes |  |
| 1x2 | confidential signer | full+spl | yes | 2286 | 26 | 3 | - | 1166 | 1010 | yes |  |
| 1x2 | confidential signer | full+data | yes | 1992 | 19 | 3 | - | 1108 | 1076 | yes |  |
| 1x2 | confidential signer | full+reply | yes | 1918 | 19 | 3 | - | 1034 | 1006 | yes |  |
| 1x2 | confidential signer | full+2tree | yes | 1956 | 20 | 3 | - | 1039 | 1010 | yes |  |
| 1x2 | confidential in-proof | min | yes | 1354 | 9 | 1 | - | 939 | 954 | yes |  |
| 1x2 | confidential in-proof | full | yes | 1917 | 18 | 2 | 1 | 1130 | 1103 | yes |  |
| 1x2 | confidential in-proof | full+spl | yes | 2285 | 25 | 2 | - | 1262 | 1107 | yes |  |
| 1x2 | confidential in-proof | full+data | yes | 1991 | 18 | 2 | - | 1204 | 1173 | yes |  |
| 1x2 | confidential in-proof | full+reply | yes | 1917 | 18 | 2 | - | 1130 | 1103 | yes |  |
| 1x2 | confidential in-proof | full+2tree | yes | 1955 | 19 | 2 | - | 1135 | 1107 | yes |  |
| 1x2 | anonymous hidden | min | yes | 1386 | 9 | 1 | - | 971 | 1026 | yes |  |
| 1x2 | anonymous hidden | full | yes | 1853 | 15 | 2 | 1 | 1162 | 1175 | yes |  |
| 1x2 | anonymous hidden | full+spl | yes | 2221 | 22 | 2 | - | 1294 | 1179 | yes |  |
| 1x2 | anonymous hidden | full+data | yes | 1927 | 15 | 2 | - | 1236 | 1245 | yes |  |
| 1x2 | anonymous hidden | full+reply | yes | 1952 | 15 | 2 | - | 1261 | 1272 | yes |  |
| 1x2 | anonymous hidden | full+2tree | yes | 1891 | 16 | 2 | - | 1167 | 1179 | yes |  |
| 1x2 | anonymous public owners | min | yes | 1323 | 9 | 1 | - | 907 | 929 | yes |  |
| 1x2 | anonymous public owners | full | yes | 1982 | 19 | 3 | 1 | 1098 | 1078 | yes |  |
| 1x2 | anonymous public owners | full+spl | yes | 2350 | 26 | 3 | - | 1230 | 1082 | yes |  |
| 1x2 | anonymous public owners | full+data | yes | 2056 | 19 | 3 | - | 1172 | 1148 | yes |  |
| 1x2 | anonymous public owners | full+reply | yes | 2081 | 19 | 3 | - | 1197 | 1175 | yes |  |
| 1x2 | anonymous public owners | full+2tree | yes | 2020 | 20 | 3 | - | 1103 | 1082 | yes |  |
| 1x2 | confidential entry | min | yes | 1403 | 9 | 1 | - | 987 | 1001 | yes |  |
| 1x2 | confidential entry | full | yes | 2062 | 19 | 3 | 1 | 1178 | 1150 | yes |  |
| 1x2 | confidential entry | full+spl | yes | 2430 | 26 | 3 | - | 1310 | 1154 | yes |  |
| 1x2 | confidential entry | full+data | yes | 2136 | 19 | 3 | - | 1252 | 1220 | yes |  |
| 1x2 | confidential entry | full+reply | yes | 2062 | 19 | 3 | - | 1178 | 1150 | yes |  |
| 1x2 | confidential entry | full+2tree | yes | 2100 | 20 | 3 | - | 1183 | 1154 | yes |  |
| 1x2 | anonymous entry | min | yes | 1562 | 9 | 1 | - | 1147 | 1170 | yes |  |
| 1x2 | anonymous entry | full | yes | 2029 | 15 | 2 | 1 | 1338 | 1319 | yes |  |
| 1x2 | anonymous entry | full+spl | yes | 2397 | 22 | 2 | - | 1470 | 1323 | yes |  |
| 1x2 | anonymous entry | full+data | yes | 2103 | 15 | 2 | - | 1412 | 1389 | yes |  |
| 1x2 | anonymous entry | full+reply | yes | 2128 | 15 | 2 | - | 1437 | 1416 | yes |  |
| 1x2 | anonymous entry | full+2tree | yes | 2067 | 16 | 2 | - | 1343 | 1323 | yes |  |
| 1x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x4 | confidential signer | min | yes | 1613 | 9 | 1 | - | 1197 | 1283 | yes |  |
| 1x4 | confidential signer | full | yes | 2272 | 19 | 3 | 1 | 1388 | 1432 | yes |  |
| 1x4 | confidential signer | full+spl | yes | 2640 | 26 | 3 | - | 1520 | 1436 | yes |  |
| 1x4 | confidential signer | full+data | yes | 2420 | 19 | 3 | - | 1536 | 1572 | yes |  |
| 1x4 | confidential signer | full+reply | yes | 2272 | 19 | 3 | - | 1388 | 1432 | yes |  |
| 1x4 | confidential signer | full+2tree | yes | 2310 | 20 | 3 | - | 1393 | 1436 | yes |  |
| 1x4 | confidential in-proof | min | yes | 1708 | 9 | 1 | - | 1293 | 1380 | yes |  |
| 1x4 | confidential in-proof | full | yes | 2271 | 18 | 2 | 1 | 1484 | 1529 | yes |  |
| 1x4 | confidential in-proof | full+spl | yes | 2639 | 25 | 2 | - | 1616 | 1533 | yes |  |
| 1x4 | confidential in-proof | full+data | yes | 2419 | 18 | 2 | - | 1632 | 1669 | yes |  |
| 1x4 | confidential in-proof | full+reply | yes | 2271 | 18 | 2 | - | 1484 | 1529 | yes |  |
| 1x4 | confidential in-proof | full+2tree | yes | 2309 | 19 | 2 | - | 1489 | 1533 | yes |  |
| 1x4 | anonymous hidden | min | yes | 1740 | 9 | 1 | - | 1325 | 1452 | yes |  |
| 1x4 | anonymous hidden | full | yes | 2207 | 15 | 2 | 1 | 1516 | 1601 | yes |  |
| 1x4 | anonymous hidden | full+spl | yes | 2575 | 22 | 2 | - | 1648 | 1605 | yes |  |
| 1x4 | anonymous hidden | full+data | yes | 2355 | 15 | 2 | - | 1664 | 1741 | yes |  |
| 1x4 | anonymous hidden | full+reply | yes | 2500 | 15 | 2 | - | 1809 | 1892 | yes |  |
| 1x4 | anonymous hidden | full+2tree | yes | 2245 | 16 | 2 | - | 1521 | 1605 | yes |  |
| 1x4 | anonymous public owners | min | yes | 1677 | 9 | 1 | - | 1261 | 1355 | yes |  |
| 1x4 | anonymous public owners | full | yes | 2336 | 19 | 3 | 1 | 1452 | 1504 | yes |  |
| 1x4 | anonymous public owners | full+spl | yes | 2704 | 26 | 3 | - | 1584 | 1508 | yes |  |
| 1x4 | anonymous public owners | full+data | yes | 2484 | 19 | 3 | - | 1600 | 1644 | yes |  |
| 1x4 | anonymous public owners | full+reply | yes | 2629 | 19 | 3 | - | 1745 | 1795 | yes |  |
| 1x4 | anonymous public owners | full+2tree | yes | 2374 | 20 | 3 | - | 1457 | 1508 | yes |  |
| 1x4 | confidential entry | min | yes | 1757 | 9 | 1 | - | 1341 | 1427 | yes |  |
| 1x4 | confidential entry | full | yes | 2416 | 19 | 3 | 1 | 1532 | 1576 | yes |  |
| 1x4 | confidential entry | full+spl | yes | 2784 | 26 | 3 | - | 1664 | 1580 | yes |  |
| 1x4 | confidential entry | full+data | yes | 2564 | 19 | 3 | - | 1680 | 1716 | yes |  |
| 1x4 | confidential entry | full+reply | yes | 2416 | 19 | 3 | - | 1532 | 1576 | yes |  |
| 1x4 | confidential entry | full+2tree | yes | 2454 | 20 | 3 | - | 1537 | 1580 | yes |  |
| 1x4 | anonymous entry | min | yes | 1916 | 9 | 1 | - | 1501 | 1596 | yes |  |
| 1x4 | anonymous entry | full | yes | 2383 | 15 | 2 | 1 | 1692 | 1745 | yes |  |
| 1x4 | anonymous entry | full+spl | yes | 2751 | 22 | 2 | - | 1824 | 1749 | yes |  |
| 1x4 | anonymous entry | full+data | yes | 2531 | 15 | 2 | - | 1840 | 1885 | yes |  |
| 1x4 | anonymous entry | full+reply | yes | 2676 | 15 | 2 | - | 1985 | 2036 | yes |  |
| 1x4 | anonymous entry | full+2tree | yes | 2421 | 16 | 2 | - | 1697 | 1749 | yes |  |
| 1x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x8 | confidential signer | min | yes | 2321 | 9 | 1 | - | 1905 | 2135 | yes |  |
| 1x8 | confidential signer | full | yes | 2980 | 19 | 3 | 1 | 2096 | 2284 | yes |  |
| 1x8 | confidential signer | full+spl | yes | 3348 | 26 | 3 | - | 2228 | 2288 | yes |  |
| 1x8 | confidential signer | full+data | yes | 3276 | 19 | 3 | - | 2392 | 2564 | yes |  |
| 1x8 | confidential signer | full+reply | yes | 2980 | 19 | 3 | - | 2096 | 2284 | yes |  |
| 1x8 | confidential signer | full+2tree | yes | 3018 | 20 | 3 | - | 2101 | 2288 | yes |  |
| 1x8 | confidential in-proof | min | yes | 2416 | 9 | 1 | - | 2001 | 2232 | yes |  |
| 1x8 | confidential in-proof | full | yes | 2979 | 18 | 2 | 1 | 2192 | 2381 | yes |  |
| 1x8 | confidential in-proof | full+spl | yes | 3347 | 25 | 2 | - | 2324 | 2385 | yes |  |
| 1x8 | confidential in-proof | full+data | yes | 3275 | 18 | 2 | - | 2488 | 2661 | yes |  |
| 1x8 | confidential in-proof | full+reply | yes | 2979 | 18 | 2 | - | 2192 | 2381 | yes |  |
| 1x8 | confidential in-proof | full+2tree | yes | 3017 | 19 | 2 | - | 2197 | 2385 | yes |  |
| 1x8 | anonymous hidden | min | yes | 2448 | 9 | 1 | - | 2033 | 2304 | yes |  |
| 1x8 | anonymous hidden | full | yes | 2915 | 15 | 2 | 1 | 2224 | 2453 | yes |  |
| 1x8 | anonymous hidden | full+spl | yes | 3283 | 22 | 2 | - | 2356 | 2457 | yes |  |
| 1x8 | anonymous hidden | full+data | yes | 3211 | 15 | 2 | - | 2520 | 2733 | yes |  |
| 1x8 | anonymous hidden | full+reply | yes | 3596 | 15 | 2 | - | 2905 | 3132 | yes |  |
| 1x8 | anonymous hidden | full+2tree | yes | 2953 | 16 | 2 | - | 2229 | 2457 | yes |  |
| 1x8 | anonymous public owners | min | yes | 2385 | 9 | 1 | - | 1969 | 2207 | yes |  |
| 1x8 | anonymous public owners | full | yes | 3044 | 19 | 3 | 1 | 2160 | 2356 | yes |  |
| 1x8 | anonymous public owners | full+spl | yes | 3412 | 26 | 3 | - | 2292 | 2360 | yes |  |
| 1x8 | anonymous public owners | full+data | yes | 3340 | 19 | 3 | - | 2456 | 2636 | yes |  |
| 1x8 | anonymous public owners | full+reply | yes | 3725 | 19 | 3 | - | 2841 | 3035 | yes |  |
| 1x8 | anonymous public owners | full+2tree | yes | 3082 | 20 | 3 | - | 2165 | 2360 | yes |  |
| 1x8 | confidential entry | min | yes | 2465 | 9 | 1 | - | 2049 | 2279 | yes |  |
| 1x8 | confidential entry | full | yes | 3124 | 19 | 3 | 1 | 2240 | 2428 | yes |  |
| 1x8 | confidential entry | full+spl | yes | 3492 | 26 | 3 | - | 2372 | 2432 | yes |  |
| 1x8 | confidential entry | full+data | yes | 3420 | 19 | 3 | - | 2536 | 2708 | yes |  |
| 1x8 | confidential entry | full+reply | yes | 3124 | 19 | 3 | - | 2240 | 2428 | yes |  |
| 1x8 | confidential entry | full+2tree | yes | 3162 | 20 | 3 | - | 2245 | 2432 | yes |  |
| 1x8 | anonymous entry | min | yes | 2624 | 9 | 1 | - | 2209 | 2448 | yes |  |
| 1x8 | anonymous entry | full | yes | 3091 | 15 | 2 | 1 | 2400 | 2597 | yes |  |
| 1x8 | anonymous entry | full+spl | yes | 3459 | 22 | 2 | - | 2532 | 2601 | yes |  |
| 1x8 | anonymous entry | full+data | yes | 3387 | 15 | 2 | - | 2696 | 2877 | yes |  |
| 1x8 | anonymous entry | full+reply | yes | 3772 | 15 | 2 | - | 3081 | 3276 | yes |  |
| 1x8 | anonymous entry | full+2tree | yes | 3129 | 16 | 2 | - | 2405 | 2601 | yes |  |
| 1x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x2 | confidential signer | min | yes | 1324 | 10 | 1 | - | 875 | 890 | yes |  |
| 2x2 | confidential signer | full | yes | 1983 | 20 | 3 | 2 | 1066 | 1039 | yes |  |
| 2x2 | confidential signer | full+spl | yes | 2351 | 27 | 3 | - | 1198 | 1043 | yes |  |
| 2x2 | confidential signer | full+data | yes | 2057 | 20 | 3 | - | 1140 | 1109 | yes |  |
| 2x2 | confidential signer | full+reply | yes | 1983 | 20 | 3 | - | 1066 | 1039 | yes |  |
| 2x2 | confidential signer | full+2tree | yes | 2021 | 21 | 3 | - | 1071 | 1043 | yes |  |
| 2x2 | confidential in-proof | min | yes | 1419 | 10 | 1 | - | 971 | 987 | yes |  |
| 2x2 | confidential in-proof | full | yes | 1982 | 19 | 2 | 2 | 1162 | 1136 | yes |  |
| 2x2 | confidential in-proof | full+spl | yes | 2350 | 26 | 2 | - | 1294 | 1140 | yes |  |
| 2x2 | confidential in-proof | full+data | yes | 2056 | 19 | 2 | - | 1236 | 1206 | yes |  |
| 2x2 | confidential in-proof | full+reply | yes | 1982 | 19 | 2 | - | 1162 | 1136 | yes |  |
| 2x2 | confidential in-proof | full+2tree | yes | 2020 | 20 | 2 | - | 1167 | 1140 | yes |  |
| 2x2 | anonymous hidden | min | yes | 1451 | 10 | 1 | - | 1003 | 1059 | yes |  |
| 2x2 | anonymous hidden | full | yes | 1918 | 16 | 2 | 2 | 1194 | 1208 | yes |  |
| 2x2 | anonymous hidden | full+spl | yes | 2286 | 23 | 2 | - | 1326 | 1212 | yes |  |
| 2x2 | anonymous hidden | full+data | yes | 1992 | 16 | 2 | - | 1268 | 1278 | yes |  |
| 2x2 | anonymous hidden | full+reply | yes | 2017 | 16 | 2 | - | 1293 | 1305 | yes |  |
| 2x2 | anonymous hidden | full+2tree | yes | 1956 | 17 | 2 | - | 1199 | 1212 | yes |  |
| 2x2 | anonymous public owners | min | yes | 1388 | 10 | 1 | - | 939 | 962 | yes |  |
| 2x2 | anonymous public owners | full | yes | 2047 | 20 | 3 | 2 | 1130 | 1111 | yes |  |
| 2x2 | anonymous public owners | full+spl | yes | 2415 | 27 | 3 | - | 1262 | 1115 | yes |  |
| 2x2 | anonymous public owners | full+data | yes | 2121 | 20 | 3 | - | 1204 | 1181 | yes |  |
| 2x2 | anonymous public owners | full+reply | yes | 2146 | 20 | 3 | - | 1229 | 1208 | yes |  |
| 2x2 | anonymous public owners | full+2tree | yes | 2085 | 21 | 3 | - | 1135 | 1115 | yes |  |
| 2x2 | confidential entry | min | yes | 1604 | 10 | 1 | - | 1155 | 1170 | yes |  |
| 2x2 | confidential entry | full | yes | 2263 | 20 | 3 | 2 | 1346 | 1319 | yes |  |
| 2x2 | confidential entry | full+spl | yes | 2631 | 27 | 3 | - | 1478 | 1323 | yes |  |
| 2x2 | confidential entry | full+data | yes | 2337 | 20 | 3 | - | 1420 | 1389 | yes |  |
| 2x2 | confidential entry | full+reply | yes | 2263 | 20 | 3 | - | 1346 | 1319 | yes |  |
| 2x2 | confidential entry | full+2tree | yes | 2301 | 21 | 3 | - | 1351 | 1323 | yes |  |
| 2x2 | anonymous entry | min | yes | 1763 | 10 | 1 | - | 1315 | 1339 | yes |  |
| 2x2 | anonymous entry | full | yes | 2230 | 16 | 2 | 2 | 1506 | 1488 | yes |  |
| 2x2 | anonymous entry | full+spl | yes | 2598 | 23 | 2 | - | 1638 | 1492 | yes |  |
| 2x2 | anonymous entry | full+data | yes | 2304 | 16 | 2 | - | 1580 | 1558 | yes |  |
| 2x2 | anonymous entry | full+reply | yes | 2329 | 16 | 2 | - | 1605 | 1585 | yes |  |
| 2x2 | anonymous entry | full+2tree | yes | 2268 | 17 | 2 | - | 1511 | 1492 | yes |  |
| 2x2 | delegate | min | yes | 1324 | 10 | 1 | - | 875 | 890 | yes |  |
| 2x2 | delegate | full | yes | 1771 | 16 | 2 | - | 1050 | 1039 | yes |  |
| 2x2 | delegate | full+spl | yes | 2011 | 23 | 2 | - | 1054 | 1043 | yes |  |
| 2x2 | delegate | full+data | yes | 1845 | 16 | 2 | - | 1124 | 1109 | yes |  |
| 2x2 | delegate | full+reply | yes | 1771 | 16 | 2 | - | 1050 | 1039 | yes |  |
| 2x2 | delegate | full+2tree | yes | 1809 | 17 | 2 | - | 1055 | 1043 | yes |  |
| 2x2 | anonymous delegate | min | yes | 1388 | 10 | 1 | - | 939 | 962 | yes |  |
| 2x2 | anonymous delegate | full | yes | 1835 | 16 | 2 | - | 1114 | 1111 | yes |  |
| 2x2 | anonymous delegate | full+spl | yes | 2075 | 23 | 2 | - | 1118 | 1115 | yes |  |
| 2x2 | anonymous delegate | full+data | yes | 1909 | 16 | 2 | - | 1188 | 1181 | yes |  |
| 2x2 | anonymous delegate | full+reply | yes | 1934 | 16 | 2 | - | 1213 | 1208 | yes |  |
| 2x2 | anonymous delegate | full+2tree | yes | 1873 | 17 | 2 | - | 1119 | 1115 | yes |  |
| 2x4 | confidential signer | min | yes | 1678 | 10 | 1 | - | 1229 | 1316 | yes |  |
| 2x4 | confidential signer | full | yes | 2337 | 20 | 3 | 2 | 1420 | 1465 | yes |  |
| 2x4 | confidential signer | full+spl | yes | 2705 | 27 | 3 | - | 1552 | 1469 | yes |  |
| 2x4 | confidential signer | full+data | yes | 2485 | 20 | 3 | - | 1568 | 1605 | yes |  |
| 2x4 | confidential signer | full+reply | yes | 2337 | 20 | 3 | - | 1420 | 1465 | yes |  |
| 2x4 | confidential signer | full+2tree | yes | 2375 | 21 | 3 | - | 1425 | 1469 | yes |  |
| 2x4 | confidential in-proof | min | yes | 1773 | 10 | 1 | - | 1325 | 1413 | yes |  |
| 2x4 | confidential in-proof | full | yes | 2336 | 19 | 2 | 2 | 1516 | 1562 | yes |  |
| 2x4 | confidential in-proof | full+spl | yes | 2704 | 26 | 2 | - | 1648 | 1566 | yes |  |
| 2x4 | confidential in-proof | full+data | yes | 2484 | 19 | 2 | - | 1664 | 1702 | yes |  |
| 2x4 | confidential in-proof | full+reply | yes | 2336 | 19 | 2 | - | 1516 | 1562 | yes |  |
| 2x4 | confidential in-proof | full+2tree | yes | 2374 | 20 | 2 | - | 1521 | 1566 | yes |  |
| 2x4 | anonymous hidden | min | yes | 1805 | 10 | 1 | - | 1357 | 1485 | yes |  |
| 2x4 | anonymous hidden | full | yes | 2272 | 16 | 2 | 2 | 1548 | 1634 | yes |  |
| 2x4 | anonymous hidden | full+spl | yes | 2640 | 23 | 2 | - | 1680 | 1638 | yes |  |
| 2x4 | anonymous hidden | full+data | yes | 2420 | 16 | 2 | - | 1696 | 1774 | yes |  |
| 2x4 | anonymous hidden | full+reply | yes | 2565 | 16 | 2 | - | 1841 | 1925 | yes |  |
| 2x4 | anonymous hidden | full+2tree | yes | 2310 | 17 | 2 | - | 1553 | 1638 | yes |  |
| 2x4 | anonymous public owners | min | yes | 1742 | 10 | 1 | - | 1293 | 1388 | yes |  |
| 2x4 | anonymous public owners | full | yes | 2401 | 20 | 3 | 2 | 1484 | 1537 | yes |  |
| 2x4 | anonymous public owners | full+spl | yes | 2769 | 27 | 3 | - | 1616 | 1541 | yes |  |
| 2x4 | anonymous public owners | full+data | yes | 2549 | 20 | 3 | - | 1632 | 1677 | yes |  |
| 2x4 | anonymous public owners | full+reply | yes | 2694 | 20 | 3 | - | 1777 | 1828 | yes |  |
| 2x4 | anonymous public owners | full+2tree | yes | 2439 | 21 | 3 | - | 1489 | 1541 | yes |  |
| 2x4 | confidential entry | min | yes | 1958 | 10 | 1 | - | 1509 | 1596 | yes |  |
| 2x4 | confidential entry | full | yes | 2617 | 20 | 3 | 2 | 1700 | 1745 | yes |  |
| 2x4 | confidential entry | full+spl | yes | 2985 | 27 | 3 | - | 1832 | 1749 | yes |  |
| 2x4 | confidential entry | full+data | yes | 2765 | 20 | 3 | - | 1848 | 1885 | yes |  |
| 2x4 | confidential entry | full+reply | yes | 2617 | 20 | 3 | - | 1700 | 1745 | yes |  |
| 2x4 | confidential entry | full+2tree | yes | 2655 | 21 | 3 | - | 1705 | 1749 | yes |  |
| 2x4 | anonymous entry | min | yes | 2117 | 10 | 1 | - | 1669 | 1765 | yes |  |
| 2x4 | anonymous entry | full | yes | 2584 | 16 | 2 | 2 | 1860 | 1914 | yes |  |
| 2x4 | anonymous entry | full+spl | yes | 2952 | 23 | 2 | - | 1992 | 1918 | yes |  |
| 2x4 | anonymous entry | full+data | yes | 2732 | 16 | 2 | - | 2008 | 2054 | yes |  |
| 2x4 | anonymous entry | full+reply | yes | 2877 | 16 | 2 | - | 2153 | 2205 | yes |  |
| 2x4 | anonymous entry | full+2tree | yes | 2622 | 17 | 2 | - | 1865 | 1918 | yes |  |
| 2x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x16 | confidential signer | min | yes | 3738 | 9 | 1 | - | 3322 | 3839 | yes |  |
| 1x16 | confidential signer | full | yes | 4397 | 19 | 3 | none | 3513 | 3988 | no | 301 B |
| 1x16 | confidential signer | full+spl | yes | 4765 | 26 | 3 | - | 3645 | 3992 | no | 669 B |
| 1x16 | confidential signer | full+data | yes | 4989 | 19 | 3 | - | 4105 | 4548 | no | 893 B |
| 1x16 | confidential signer | full+reply | yes | 4397 | 19 | 3 | - | 3513 | 3988 | no | 301 B |
| 1x16 | confidential signer | full+2tree | yes | 4435 | 20 | 3 | - | 3518 | 3992 | no | 339 B |
| 1x16 | confidential in-proof | min | yes | 3833 | 9 | 1 | - | 3418 | 3936 | yes |  |
| 1x16 | confidential in-proof | full | yes | 4396 | 18 | 2 | none | 3609 | 4085 | no | 300 B |
| 1x16 | confidential in-proof | full+spl | yes | 4764 | 25 | 2 | - | 3741 | 4089 | no | 668 B |
| 1x16 | confidential in-proof | full+data | yes | 4988 | 18 | 2 | - | 4201 | 4645 | no | 892 B |
| 1x16 | confidential in-proof | full+reply | yes | 4396 | 18 | 2 | - | 3609 | 4085 | no | 300 B |
| 1x16 | confidential in-proof | full+2tree | yes | 4434 | 19 | 2 | - | 3614 | 4089 | no | 338 B |
| 1x16 | anonymous hidden | min | yes | 3865 | 9 | 1 | - | 3450 | 4008 | yes |  |
| 1x16 | anonymous hidden | full | yes | 4332 | 15 | 2 | none | 3641 | 4157 | no | 236 B |
| 1x16 | anonymous hidden | full+spl | yes | 4700 | 22 | 2 | - | 3773 | 4161 | no | 604 B |
| 1x16 | anonymous hidden | full+data | yes | 4924 | 15 | 2 | - | 4233 | 4717 | no | 828 B |
| 1x16 | anonymous hidden | full+reply | yes | 5789 | 15 | 2 | - | 5098 | 5612 | no | 1693 B |
| 1x16 | anonymous hidden | full+2tree | yes | 4370 | 16 | 2 | - | 3646 | 4161 | no | 274 B |
| 1x16 | anonymous public owners | min | yes | 3802 | 9 | 1 | - | 3386 | 3911 | yes |  |
| 1x16 | anonymous public owners | full | yes | 4461 | 19 | 3 | none | 3577 | 4060 | no | 365 B |
| 1x16 | anonymous public owners | full+spl | yes | 4829 | 26 | 3 | - | 3709 | 4064 | no | 733 B |
| 1x16 | anonymous public owners | full+data | yes | 5053 | 19 | 3 | - | 4169 | 4620 | no | 957 B |
| 1x16 | anonymous public owners | full+reply | yes | 5918 | 19 | 3 | - | 5034 | 5515 | no | 1822 B |
| 1x16 | anonymous public owners | full+2tree | yes | 4499 | 20 | 3 | - | 3582 | 4064 | no | 403 B |
| 1x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x8 | confidential signer | min | yes | 2386 | 10 | 1 | - | 1937 | 2168 | yes |  |
| 2x8 | confidential signer | full | yes | 3045 | 20 | 3 | 2 | 2128 | 2317 | yes |  |
| 2x8 | confidential signer | full+spl | yes | 3413 | 27 | 3 | - | 2260 | 2321 | yes |  |
| 2x8 | confidential signer | full+data | yes | 3341 | 20 | 3 | - | 2424 | 2597 | yes |  |
| 2x8 | confidential signer | full+reply | yes | 3045 | 20 | 3 | - | 2128 | 2317 | yes |  |
| 2x8 | confidential signer | full+2tree | yes | 3083 | 21 | 3 | - | 2133 | 2321 | yes |  |
| 2x8 | confidential in-proof | min | yes | 2481 | 10 | 1 | - | 2033 | 2265 | yes |  |
| 2x8 | confidential in-proof | full | yes | 3044 | 19 | 2 | 2 | 2224 | 2414 | yes |  |
| 2x8 | confidential in-proof | full+spl | yes | 3412 | 26 | 2 | - | 2356 | 2418 | yes |  |
| 2x8 | confidential in-proof | full+data | yes | 3340 | 19 | 2 | - | 2520 | 2694 | yes |  |
| 2x8 | confidential in-proof | full+reply | yes | 3044 | 19 | 2 | - | 2224 | 2414 | yes |  |
| 2x8 | confidential in-proof | full+2tree | yes | 3082 | 20 | 2 | - | 2229 | 2418 | yes |  |
| 2x8 | anonymous hidden | min | yes | 2513 | 10 | 1 | - | 2065 | 2337 | yes |  |
| 2x8 | anonymous hidden | full | yes | 2980 | 16 | 2 | 2 | 2256 | 2486 | yes |  |
| 2x8 | anonymous hidden | full+spl | yes | 3348 | 23 | 2 | - | 2388 | 2490 | yes |  |
| 2x8 | anonymous hidden | full+data | yes | 3276 | 16 | 2 | - | 2552 | 2766 | yes |  |
| 2x8 | anonymous hidden | full+reply | yes | 3661 | 16 | 2 | - | 2937 | 3165 | yes |  |
| 2x8 | anonymous hidden | full+2tree | yes | 3018 | 17 | 2 | - | 2261 | 2490 | yes |  |
| 2x8 | anonymous public owners | min | yes | 2450 | 10 | 1 | - | 2001 | 2240 | yes |  |
| 2x8 | anonymous public owners | full | yes | 3109 | 20 | 3 | 2 | 2192 | 2389 | yes |  |
| 2x8 | anonymous public owners | full+spl | yes | 3477 | 27 | 3 | - | 2324 | 2393 | yes |  |
| 2x8 | anonymous public owners | full+data | yes | 3405 | 20 | 3 | - | 2488 | 2669 | yes |  |
| 2x8 | anonymous public owners | full+reply | yes | 3790 | 20 | 3 | - | 2873 | 3068 | yes |  |
| 2x8 | anonymous public owners | full+2tree | yes | 3147 | 21 | 3 | - | 2197 | 2393 | yes |  |
| 2x8 | confidential entry | min | yes | 2666 | 10 | 1 | - | 2217 | 2448 | yes |  |
| 2x8 | confidential entry | full | yes | 3325 | 20 | 3 | 2 | 2408 | 2597 | yes |  |
| 2x8 | confidential entry | full+spl | yes | 3693 | 27 | 3 | - | 2540 | 2601 | yes |  |
| 2x8 | confidential entry | full+data | yes | 3621 | 20 | 3 | - | 2704 | 2877 | yes |  |
| 2x8 | confidential entry | full+reply | yes | 3325 | 20 | 3 | - | 2408 | 2597 | yes |  |
| 2x8 | confidential entry | full+2tree | yes | 3363 | 21 | 3 | - | 2413 | 2601 | yes |  |
| 2x8 | anonymous entry | min | yes | 2825 | 10 | 1 | - | 2377 | 2617 | yes |  |
| 2x8 | anonymous entry | full | yes | 3292 | 16 | 2 | 2 | 2568 | 2766 | yes |  |
| 2x8 | anonymous entry | full+spl | yes | 3660 | 23 | 2 | - | 2700 | 2770 | yes |  |
| 2x8 | anonymous entry | full+data | yes | 3588 | 16 | 2 | - | 2864 | 3046 | yes |  |
| 2x8 | anonymous entry | full+reply | yes | 3973 | 16 | 2 | - | 3249 | 3445 | yes |  |
| 2x8 | anonymous entry | full+2tree | yes | 3330 | 17 | 2 | - | 2573 | 2770 | yes |  |
| 2x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x2 | confidential signer | min | yes | 1389 | 11 | 1 | - | 907 | 923 | yes |  |
| 3x2 | confidential signer | full | yes | 2048 | 21 | 3 | 3 | 1098 | 1072 | yes |  |
| 3x2 | confidential signer | full+spl | yes | 2416 | 28 | 3 | - | 1230 | 1076 | yes |  |
| 3x2 | confidential signer | full+data | yes | 2122 | 21 | 3 | - | 1172 | 1142 | yes |  |
| 3x2 | confidential signer | full+reply | yes | 2048 | 21 | 3 | - | 1098 | 1072 | yes |  |
| 3x2 | confidential signer | full+2tree | yes | 2086 | 22 | 3 | - | 1103 | 1076 | yes |  |
| 3x2 | confidential in-proof | min | yes | 1484 | 11 | 1 | - | 1003 | 1020 | yes |  |
| 3x2 | confidential in-proof | full | yes | 2047 | 20 | 2 | 3 | 1194 | 1169 | yes |  |
| 3x2 | confidential in-proof | full+spl | yes | 2415 | 27 | 2 | - | 1326 | 1173 | yes |  |
| 3x2 | confidential in-proof | full+data | yes | 2121 | 20 | 2 | - | 1268 | 1239 | yes |  |
| 3x2 | confidential in-proof | full+reply | yes | 2047 | 20 | 2 | - | 1194 | 1169 | yes |  |
| 3x2 | confidential in-proof | full+2tree | yes | 2085 | 21 | 2 | - | 1199 | 1173 | yes |  |
| 3x2 | anonymous hidden | min | yes | 1516 | 11 | 1 | - | 1035 | 1092 | yes |  |
| 3x2 | anonymous hidden | full | yes | 1983 | 17 | 2 | 3 | 1226 | 1241 | yes |  |
| 3x2 | anonymous hidden | full+spl | yes | 2351 | 24 | 2 | - | 1358 | 1245 | yes |  |
| 3x2 | anonymous hidden | full+data | yes | 2057 | 17 | 2 | - | 1300 | 1311 | yes |  |
| 3x2 | anonymous hidden | full+reply | yes | 2082 | 17 | 2 | - | 1325 | 1338 | yes |  |
| 3x2 | anonymous hidden | full+2tree | yes | 2021 | 18 | 2 | - | 1231 | 1245 | yes |  |
| 3x2 | anonymous public owners | min | yes | 1453 | 11 | 1 | - | 971 | 995 | yes |  |
| 3x2 | anonymous public owners | full | yes | 2112 | 21 | 3 | 3 | 1162 | 1144 | yes |  |
| 3x2 | anonymous public owners | full+spl | yes | 2480 | 28 | 3 | - | 1294 | 1148 | yes |  |
| 3x2 | anonymous public owners | full+data | yes | 2186 | 21 | 3 | - | 1236 | 1214 | yes |  |
| 3x2 | anonymous public owners | full+reply | yes | 2211 | 21 | 3 | - | 1261 | 1241 | yes |  |
| 3x2 | anonymous public owners | full+2tree | yes | 2150 | 22 | 3 | - | 1167 | 1148 | yes |  |
| 3x2 | confidential entry | min | yes | 1805 | 11 | 1 | - | 1323 | 1339 | yes |  |
| 3x2 | confidential entry | full | yes | 2464 | 21 | 3 | 3 | 1514 | 1488 | yes |  |
| 3x2 | confidential entry | full+spl | yes | 2832 | 28 | 3 | - | 1646 | 1492 | yes |  |
| 3x2 | confidential entry | full+data | yes | 2538 | 21 | 3 | - | 1588 | 1558 | yes |  |
| 3x2 | confidential entry | full+reply | yes | 2464 | 21 | 3 | - | 1514 | 1488 | yes |  |
| 3x2 | confidential entry | full+2tree | yes | 2502 | 22 | 3 | - | 1519 | 1492 | yes |  |
| 3x2 | anonymous entry | min | yes | 1964 | 11 | 1 | - | 1483 | 1508 | yes |  |
| 3x2 | anonymous entry | full | yes | 2431 | 17 | 2 | 3 | 1674 | 1657 | yes |  |
| 3x2 | anonymous entry | full+spl | yes | 2799 | 24 | 2 | - | 1806 | 1661 | yes |  |
| 3x2 | anonymous entry | full+data | yes | 2505 | 17 | 2 | - | 1748 | 1727 | yes |  |
| 3x2 | anonymous entry | full+reply | yes | 2530 | 17 | 2 | - | 1773 | 1754 | yes |  |
| 3x2 | anonymous entry | full+2tree | yes | 2469 | 18 | 2 | - | 1679 | 1661 | yes |  |
| 3x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x4 | confidential signer | min | yes | 1743 | 11 | 1 | - | 1261 | 1349 | yes |  |
| 3x4 | confidential signer | full | yes | 2402 | 21 | 3 | 3 | 1452 | 1498 | yes |  |
| 3x4 | confidential signer | full+spl | yes | 2770 | 28 | 3 | - | 1584 | 1502 | yes |  |
| 3x4 | confidential signer | full+data | yes | 2550 | 21 | 3 | - | 1600 | 1638 | yes |  |
| 3x4 | confidential signer | full+reply | yes | 2402 | 21 | 3 | - | 1452 | 1498 | yes |  |
| 3x4 | confidential signer | full+2tree | yes | 2440 | 22 | 3 | - | 1457 | 1502 | yes |  |
| 3x4 | confidential in-proof | min | yes | 1838 | 11 | 1 | - | 1357 | 1446 | yes |  |
| 3x4 | confidential in-proof | full | yes | 2401 | 20 | 2 | 3 | 1548 | 1595 | yes |  |
| 3x4 | confidential in-proof | full+spl | yes | 2769 | 27 | 2 | - | 1680 | 1599 | yes |  |
| 3x4 | confidential in-proof | full+data | yes | 2549 | 20 | 2 | - | 1696 | 1735 | yes |  |
| 3x4 | confidential in-proof | full+reply | yes | 2401 | 20 | 2 | - | 1548 | 1595 | yes |  |
| 3x4 | confidential in-proof | full+2tree | yes | 2439 | 21 | 2 | - | 1553 | 1599 | yes |  |
| 3x4 | anonymous hidden | min | yes | 1870 | 11 | 1 | - | 1389 | 1518 | yes |  |
| 3x4 | anonymous hidden | full | yes | 2337 | 17 | 2 | 3 | 1580 | 1667 | yes |  |
| 3x4 | anonymous hidden | full+spl | yes | 2705 | 24 | 2 | - | 1712 | 1671 | yes |  |
| 3x4 | anonymous hidden | full+data | yes | 2485 | 17 | 2 | - | 1728 | 1807 | yes |  |
| 3x4 | anonymous hidden | full+reply | yes | 2630 | 17 | 2 | - | 1873 | 1958 | yes |  |
| 3x4 | anonymous hidden | full+2tree | yes | 2375 | 18 | 2 | - | 1585 | 1671 | yes |  |
| 3x4 | anonymous public owners | min | yes | 1807 | 11 | 1 | - | 1325 | 1421 | yes |  |
| 3x4 | anonymous public owners | full | yes | 2466 | 21 | 3 | 3 | 1516 | 1570 | yes |  |
| 3x4 | anonymous public owners | full+spl | yes | 2834 | 28 | 3 | - | 1648 | 1574 | yes |  |
| 3x4 | anonymous public owners | full+data | yes | 2614 | 21 | 3 | - | 1664 | 1710 | yes |  |
| 3x4 | anonymous public owners | full+reply | yes | 2759 | 21 | 3 | - | 1809 | 1861 | yes |  |
| 3x4 | anonymous public owners | full+2tree | yes | 2504 | 22 | 3 | - | 1521 | 1574 | yes |  |
| 3x4 | confidential entry | min | yes | 2159 | 11 | 1 | - | 1677 | 1765 | yes |  |
| 3x4 | confidential entry | full | yes | 2818 | 21 | 3 | 3 | 1868 | 1914 | yes |  |
| 3x4 | confidential entry | full+spl | yes | 3186 | 28 | 3 | - | 2000 | 1918 | yes |  |
| 3x4 | confidential entry | full+data | yes | 2966 | 21 | 3 | - | 2016 | 2054 | yes |  |
| 3x4 | confidential entry | full+reply | yes | 2818 | 21 | 3 | - | 1868 | 1914 | yes |  |
| 3x4 | confidential entry | full+2tree | yes | 2856 | 22 | 3 | - | 1873 | 1918 | yes |  |
| 3x4 | anonymous entry | min | yes | 2318 | 11 | 1 | - | 1837 | 1934 | yes |  |
| 3x4 | anonymous entry | full | yes | 2785 | 17 | 2 | 3 | 2028 | 2083 | yes |  |
| 3x4 | anonymous entry | full+spl | yes | 3153 | 24 | 2 | - | 2160 | 2087 | yes |  |
| 3x4 | anonymous entry | full+data | yes | 2933 | 17 | 2 | - | 2176 | 2223 | yes |  |
| 3x4 | anonymous entry | full+reply | yes | 3078 | 17 | 2 | - | 2321 | 2374 | yes |  |
| 3x4 | anonymous entry | full+2tree | yes | 2823 | 18 | 2 | - | 2033 | 2087 | yes |  |
| 3x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x16 | confidential signer | min | yes | 3803 | 10 | 1 | - | 3354 | 3872 | yes |  |
| 2x16 | confidential signer | full | yes | 4462 | 20 | 3 | none | 3545 | 4021 | no | 366 B |
| 2x16 | confidential signer | full+spl | yes | 4830 | 27 | 3 | - | 3677 | 4025 | no | 734 B |
| 2x16 | confidential signer | full+data | yes | 5054 | 20 | 3 | - | 4137 | 4581 | no | 958 B |
| 2x16 | confidential signer | full+reply | yes | 4462 | 20 | 3 | - | 3545 | 4021 | no | 366 B |
| 2x16 | confidential signer | full+2tree | yes | 4500 | 21 | 3 | - | 3550 | 4025 | no | 404 B |
| 2x16 | confidential in-proof | min | yes | 3898 | 10 | 1 | - | 3450 | 3969 | yes |  |
| 2x16 | confidential in-proof | full | yes | 4461 | 19 | 2 | none | 3641 | 4118 | no | 365 B |
| 2x16 | confidential in-proof | full+spl | yes | 4829 | 26 | 2 | - | 3773 | 4122 | no | 733 B |
| 2x16 | confidential in-proof | full+data | yes | 5053 | 19 | 2 | - | 4233 | 4678 | no | 957 B |
| 2x16 | confidential in-proof | full+reply | yes | 4461 | 19 | 2 | - | 3641 | 4118 | no | 365 B |
| 2x16 | confidential in-proof | full+2tree | yes | 4499 | 20 | 2 | - | 3646 | 4122 | no | 403 B |
| 2x16 | anonymous hidden | min | yes | 3930 | 10 | 1 | - | 3482 | 4041 | yes |  |
| 2x16 | anonymous hidden | full | yes | 4397 | 16 | 2 | none | 3673 | 4190 | no | 301 B |
| 2x16 | anonymous hidden | full+spl | yes | 4765 | 23 | 2 | - | 3805 | 4194 | no | 669 B |
| 2x16 | anonymous hidden | full+data | yes | 4989 | 16 | 2 | - | 4265 | 4750 | no | 893 B |
| 2x16 | anonymous hidden | full+reply | yes | 5854 | 16 | 2 | - | 5130 | 5645 | no | 1758 B |
| 2x16 | anonymous hidden | full+2tree | yes | 4435 | 17 | 2 | - | 3678 | 4194 | no | 339 B |
| 2x16 | anonymous public owners | min | yes | 3867 | 10 | 1 | - | 3418 | 3944 | yes |  |
| 2x16 | anonymous public owners | full | yes | 4526 | 20 | 3 | none | 3609 | 4093 | no | 430 B |
| 2x16 | anonymous public owners | full+spl | yes | 4894 | 27 | 3 | - | 3741 | 4097 | no | 798 B |
| 2x16 | anonymous public owners | full+data | yes | 5118 | 20 | 3 | - | 4201 | 4653 | no | 1022 B |
| 2x16 | anonymous public owners | full+reply | yes | 5983 | 20 | 3 | - | 5066 | 5548 | no | 1887 B |
| 2x16 | anonymous public owners | full+2tree | yes | 4564 | 21 | 3 | - | 3614 | 4097 | no | 468 B |
| 2x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x8 | confidential signer | min | yes | 2451 | 11 | 1 | - | 1969 | 2201 | yes |  |
| 3x8 | confidential signer | full | yes | 3110 | 21 | 3 | 3 | 2160 | 2350 | yes |  |
| 3x8 | confidential signer | full+spl | yes | 3478 | 28 | 3 | - | 2292 | 2354 | yes |  |
| 3x8 | confidential signer | full+data | yes | 3406 | 21 | 3 | - | 2456 | 2630 | yes |  |
| 3x8 | confidential signer | full+reply | yes | 3110 | 21 | 3 | - | 2160 | 2350 | yes |  |
| 3x8 | confidential signer | full+2tree | yes | 3148 | 22 | 3 | - | 2165 | 2354 | yes |  |
| 3x8 | confidential in-proof | min | yes | 2546 | 11 | 1 | - | 2065 | 2298 | yes |  |
| 3x8 | confidential in-proof | full | yes | 3109 | 20 | 2 | 3 | 2256 | 2447 | yes |  |
| 3x8 | confidential in-proof | full+spl | yes | 3477 | 27 | 2 | - | 2388 | 2451 | yes |  |
| 3x8 | confidential in-proof | full+data | yes | 3405 | 20 | 2 | - | 2552 | 2727 | yes |  |
| 3x8 | confidential in-proof | full+reply | yes | 3109 | 20 | 2 | - | 2256 | 2447 | yes |  |
| 3x8 | confidential in-proof | full+2tree | yes | 3147 | 21 | 2 | - | 2261 | 2451 | yes |  |
| 3x8 | anonymous hidden | min | yes | 2578 | 11 | 1 | - | 2097 | 2370 | yes |  |
| 3x8 | anonymous hidden | full | yes | 3045 | 17 | 2 | 3 | 2288 | 2519 | yes |  |
| 3x8 | anonymous hidden | full+spl | yes | 3413 | 24 | 2 | - | 2420 | 2523 | yes |  |
| 3x8 | anonymous hidden | full+data | yes | 3341 | 17 | 2 | - | 2584 | 2799 | yes |  |
| 3x8 | anonymous hidden | full+reply | yes | 3726 | 17 | 2 | - | 2969 | 3198 | yes |  |
| 3x8 | anonymous hidden | full+2tree | yes | 3083 | 18 | 2 | - | 2293 | 2523 | yes |  |
| 3x8 | anonymous public owners | min | yes | 2515 | 11 | 1 | - | 2033 | 2273 | yes |  |
| 3x8 | anonymous public owners | full | yes | 3174 | 21 | 3 | 3 | 2224 | 2422 | yes |  |
| 3x8 | anonymous public owners | full+spl | yes | 3542 | 28 | 3 | - | 2356 | 2426 | yes |  |
| 3x8 | anonymous public owners | full+data | yes | 3470 | 21 | 3 | - | 2520 | 2702 | yes |  |
| 3x8 | anonymous public owners | full+reply | yes | 3855 | 21 | 3 | - | 2905 | 3101 | yes |  |
| 3x8 | anonymous public owners | full+2tree | yes | 3212 | 22 | 3 | - | 2229 | 2426 | yes |  |
| 3x8 | confidential entry | min | yes | 2867 | 11 | 1 | - | 2385 | 2617 | yes |  |
| 3x8 | confidential entry | full | yes | 3526 | 21 | 3 | 3 | 2576 | 2766 | yes |  |
| 3x8 | confidential entry | full+spl | yes | 3894 | 28 | 3 | - | 2708 | 2770 | yes |  |
| 3x8 | confidential entry | full+data | yes | 3822 | 21 | 3 | - | 2872 | 3046 | yes |  |
| 3x8 | confidential entry | full+reply | yes | 3526 | 21 | 3 | - | 2576 | 2766 | yes |  |
| 3x8 | confidential entry | full+2tree | yes | 3564 | 22 | 3 | - | 2581 | 2770 | yes |  |
| 3x8 | anonymous entry | min | yes | 3026 | 11 | 1 | - | 2545 | 2786 | yes |  |
| 3x8 | anonymous entry | full | yes | 3493 | 17 | 2 | 3 | 2736 | 2935 | yes |  |
| 3x8 | anonymous entry | full+spl | yes | 3861 | 24 | 2 | - | 2868 | 2939 | yes |  |
| 3x8 | anonymous entry | full+data | yes | 3789 | 17 | 2 | - | 3032 | 3215 | yes |  |
| 3x8 | anonymous entry | full+reply | yes | 4174 | 17 | 2 | - | 3417 | 3614 | no | 78 B |
| 3x8 | anonymous entry | full+2tree | yes | 3531 | 18 | 2 | - | 2741 | 2939 | yes |  |
| 3x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x2 | confidential signer | min | yes | 1454 | 12 | 1 | - | 939 | 956 | yes |  |
| 4x2 | confidential signer | full | yes | 2113 | 22 | 3 | 4 | 1130 | 1105 | yes |  |
| 4x2 | confidential signer | full+spl | yes | 2481 | 29 | 3 | - | 1262 | 1109 | yes |  |
| 4x2 | confidential signer | full+data | yes | 2187 | 22 | 3 | - | 1204 | 1175 | yes |  |
| 4x2 | confidential signer | full+reply | yes | 2113 | 22 | 3 | - | 1130 | 1105 | yes |  |
| 4x2 | confidential signer | full+2tree | yes | 2151 | 23 | 3 | - | 1135 | 1109 | yes |  |
| 4x2 | confidential in-proof | min | yes | 1549 | 12 | 1 | - | 1035 | 1053 | yes |  |
| 4x2 | confidential in-proof | full | yes | 2112 | 21 | 2 | 4 | 1226 | 1202 | yes |  |
| 4x2 | confidential in-proof | full+spl | yes | 2480 | 28 | 2 | - | 1358 | 1206 | yes |  |
| 4x2 | confidential in-proof | full+data | yes | 2186 | 21 | 2 | - | 1300 | 1272 | yes |  |
| 4x2 | confidential in-proof | full+reply | yes | 2112 | 21 | 2 | - | 1226 | 1202 | yes |  |
| 4x2 | confidential in-proof | full+2tree | yes | 2150 | 22 | 2 | - | 1231 | 1206 | yes |  |
| 4x2 | anonymous hidden | min | yes | 1581 | 12 | 1 | - | 1067 | 1125 | yes |  |
| 4x2 | anonymous hidden | full | yes | 2048 | 18 | 2 | 4 | 1258 | 1274 | yes |  |
| 4x2 | anonymous hidden | full+spl | yes | 2416 | 25 | 2 | - | 1390 | 1278 | yes |  |
| 4x2 | anonymous hidden | full+data | yes | 2122 | 18 | 2 | - | 1332 | 1344 | yes |  |
| 4x2 | anonymous hidden | full+reply | yes | 2147 | 18 | 2 | - | 1357 | 1371 | yes |  |
| 4x2 | anonymous hidden | full+2tree | yes | 2086 | 19 | 2 | - | 1263 | 1278 | yes |  |
| 4x2 | anonymous public owners | min | yes | 1518 | 12 | 1 | - | 1003 | 1028 | yes |  |
| 4x2 | anonymous public owners | full | yes | 2177 | 22 | 3 | 4 | 1194 | 1177 | yes |  |
| 4x2 | anonymous public owners | full+spl | yes | 2545 | 29 | 3 | - | 1326 | 1181 | yes |  |
| 4x2 | anonymous public owners | full+data | yes | 2251 | 22 | 3 | - | 1268 | 1247 | yes |  |
| 4x2 | anonymous public owners | full+reply | yes | 2276 | 22 | 3 | - | 1293 | 1274 | yes |  |
| 4x2 | anonymous public owners | full+2tree | yes | 2215 | 23 | 3 | - | 1199 | 1181 | yes |  |
| 4x2 | confidential entry | min | yes | 2006 | 12 | 1 | - | 1491 | 1508 | yes |  |
| 4x2 | confidential entry | full | yes | 2665 | 22 | 3 | 4 | 1682 | 1657 | yes |  |
| 4x2 | confidential entry | full+spl | yes | 3033 | 29 | 3 | - | 1814 | 1661 | yes |  |
| 4x2 | confidential entry | full+data | yes | 2739 | 22 | 3 | - | 1756 | 1727 | yes |  |
| 4x2 | confidential entry | full+reply | yes | 2665 | 22 | 3 | - | 1682 | 1657 | yes |  |
| 4x2 | confidential entry | full+2tree | yes | 2703 | 23 | 3 | - | 1687 | 1661 | yes |  |
| 4x2 | anonymous entry | min | yes | 2165 | 12 | 1 | - | 1651 | 1677 | yes |  |
| 4x2 | anonymous entry | full | yes | 2632 | 18 | 2 | 4 | 1842 | 1826 | yes |  |
| 4x2 | anonymous entry | full+spl | yes | 3000 | 25 | 2 | - | 1974 | 1830 | yes |  |
| 4x2 | anonymous entry | full+data | yes | 2706 | 18 | 2 | - | 1916 | 1896 | yes |  |
| 4x2 | anonymous entry | full+reply | yes | 2731 | 18 | 2 | - | 1941 | 1923 | yes |  |
| 4x2 | anonymous entry | full+2tree | yes | 2670 | 19 | 2 | - | 1847 | 1830 | yes |  |
| 4x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x4 | confidential signer | min | yes | 1808 | 12 | 1 | - | 1293 | 1382 | yes |  |
| 4x4 | confidential signer | full | yes | 2467 | 22 | 3 | 4 | 1484 | 1531 | yes |  |
| 4x4 | confidential signer | full+spl | yes | 2835 | 29 | 3 | - | 1616 | 1535 | yes |  |
| 4x4 | confidential signer | full+data | yes | 2615 | 22 | 3 | - | 1632 | 1671 | yes |  |
| 4x4 | confidential signer | full+reply | yes | 2467 | 22 | 3 | - | 1484 | 1531 | yes |  |
| 4x4 | confidential signer | full+2tree | yes | 2505 | 23 | 3 | - | 1489 | 1535 | yes |  |
| 4x4 | confidential in-proof | min | yes | 1903 | 12 | 1 | - | 1389 | 1479 | yes |  |
| 4x4 | confidential in-proof | full | yes | 2466 | 21 | 2 | 4 | 1580 | 1628 | yes |  |
| 4x4 | confidential in-proof | full+spl | yes | 2834 | 28 | 2 | - | 1712 | 1632 | yes |  |
| 4x4 | confidential in-proof | full+data | yes | 2614 | 21 | 2 | - | 1728 | 1768 | yes |  |
| 4x4 | confidential in-proof | full+reply | yes | 2466 | 21 | 2 | - | 1580 | 1628 | yes |  |
| 4x4 | confidential in-proof | full+2tree | yes | 2504 | 22 | 2 | - | 1585 | 1632 | yes |  |
| 4x4 | anonymous hidden | min | yes | 1935 | 12 | 1 | - | 1421 | 1551 | yes |  |
| 4x4 | anonymous hidden | full | yes | 2402 | 18 | 2 | 4 | 1612 | 1700 | yes |  |
| 4x4 | anonymous hidden | full+spl | yes | 2770 | 25 | 2 | - | 1744 | 1704 | yes |  |
| 4x4 | anonymous hidden | full+data | yes | 2550 | 18 | 2 | - | 1760 | 1840 | yes |  |
| 4x4 | anonymous hidden | full+reply | yes | 2695 | 18 | 2 | - | 1905 | 1991 | yes |  |
| 4x4 | anonymous hidden | full+2tree | yes | 2440 | 19 | 2 | - | 1617 | 1704 | yes |  |
| 4x4 | anonymous public owners | min | yes | 1872 | 12 | 1 | - | 1357 | 1454 | yes |  |
| 4x4 | anonymous public owners | full | yes | 2531 | 22 | 3 | 4 | 1548 | 1603 | yes |  |
| 4x4 | anonymous public owners | full+spl | yes | 2899 | 29 | 3 | - | 1680 | 1607 | yes |  |
| 4x4 | anonymous public owners | full+data | yes | 2679 | 22 | 3 | - | 1696 | 1743 | yes |  |
| 4x4 | anonymous public owners | full+reply | yes | 2824 | 22 | 3 | - | 1841 | 1894 | yes |  |
| 4x4 | anonymous public owners | full+2tree | yes | 2569 | 23 | 3 | - | 1553 | 1607 | yes |  |
| 4x4 | confidential entry | min | yes | 2360 | 12 | 1 | - | 1845 | 1934 | yes |  |
| 4x4 | confidential entry | full | yes | 3019 | 22 | 3 | 4 | 2036 | 2083 | yes |  |
| 4x4 | confidential entry | full+spl | yes | 3387 | 29 | 3 | - | 2168 | 2087 | yes |  |
| 4x4 | confidential entry | full+data | yes | 3167 | 22 | 3 | - | 2184 | 2223 | yes |  |
| 4x4 | confidential entry | full+reply | yes | 3019 | 22 | 3 | - | 2036 | 2083 | yes |  |
| 4x4 | confidential entry | full+2tree | yes | 3057 | 23 | 3 | - | 2041 | 2087 | yes |  |
| 4x4 | anonymous entry | min | yes | 2519 | 12 | 1 | - | 2005 | 2103 | yes |  |
| 4x4 | anonymous entry | full | yes | 2986 | 18 | 2 | 4 | 2196 | 2252 | yes |  |
| 4x4 | anonymous entry | full+spl | yes | 3354 | 25 | 2 | - | 2328 | 2256 | yes |  |
| 4x4 | anonymous entry | full+data | yes | 3134 | 18 | 2 | - | 2344 | 2392 | yes |  |
| 4x4 | anonymous entry | full+reply | yes | 3279 | 18 | 2 | - | 2489 | 2543 | yes |  |
| 4x4 | anonymous entry | full+2tree | yes | 3024 | 19 | 2 | - | 2201 | 2256 | yes |  |
| 4x4 | delegate | min | yes | 1808 | 12 | 1 | - | 1293 | 1382 | yes |  |
| 4x4 | delegate | full | yes | 2255 | 18 | 2 | - | 1468 | 1531 | yes |  |
| 4x4 | delegate | full+spl | yes | 2495 | 25 | 2 | - | 1472 | 1535 | yes |  |
| 4x4 | delegate | full+data | yes | 2403 | 18 | 2 | - | 1616 | 1671 | yes |  |
| 4x4 | delegate | full+reply | yes | 2255 | 18 | 2 | - | 1468 | 1531 | yes |  |
| 4x4 | delegate | full+2tree | yes | 2293 | 19 | 2 | - | 1473 | 1535 | yes |  |
| 4x4 | anonymous delegate | min | yes | 1872 | 12 | 1 | - | 1357 | 1454 | yes |  |
| 4x4 | anonymous delegate | full | yes | 2319 | 18 | 2 | - | 1532 | 1603 | yes |  |
| 4x4 | anonymous delegate | full+spl | yes | 2559 | 25 | 2 | - | 1536 | 1607 | yes |  |
| 4x4 | anonymous delegate | full+data | yes | 2467 | 18 | 2 | - | 1680 | 1743 | yes |  |
| 4x4 | anonymous delegate | full+reply | yes | 2612 | 18 | 2 | - | 1825 | 1894 | yes |  |
| 4x4 | anonymous delegate | full+2tree | yes | 2357 | 19 | 2 | - | 1537 | 1607 | yes |  |
| 4x8 | confidential signer | min | yes | 2516 | 12 | 1 | - | 2001 | 2234 | yes |  |
| 4x8 | confidential signer | full | yes | 3175 | 22 | 3 | 4 | 2192 | 2383 | yes |  |
| 4x8 | confidential signer | full+spl | yes | 3543 | 29 | 3 | - | 2324 | 2387 | yes |  |
| 4x8 | confidential signer | full+data | yes | 3471 | 22 | 3 | - | 2488 | 2663 | yes |  |
| 4x8 | confidential signer | full+reply | yes | 3175 | 22 | 3 | - | 2192 | 2383 | yes |  |
| 4x8 | confidential signer | full+2tree | yes | 3213 | 23 | 3 | - | 2197 | 2387 | yes |  |
| 4x8 | confidential in-proof | min | yes | 2611 | 12 | 1 | - | 2097 | 2331 | yes |  |
| 4x8 | confidential in-proof | full | yes | 3174 | 21 | 2 | 4 | 2288 | 2480 | yes |  |
| 4x8 | confidential in-proof | full+spl | yes | 3542 | 28 | 2 | - | 2420 | 2484 | yes |  |
| 4x8 | confidential in-proof | full+data | yes | 3470 | 21 | 2 | - | 2584 | 2760 | yes |  |
| 4x8 | confidential in-proof | full+reply | yes | 3174 | 21 | 2 | - | 2288 | 2480 | yes |  |
| 4x8 | confidential in-proof | full+2tree | yes | 3212 | 22 | 2 | - | 2293 | 2484 | yes |  |
| 4x8 | anonymous hidden | min | yes | 2643 | 12 | 1 | - | 2129 | 2403 | yes |  |
| 4x8 | anonymous hidden | full | yes | 3110 | 18 | 2 | 4 | 2320 | 2552 | yes |  |
| 4x8 | anonymous hidden | full+spl | yes | 3478 | 25 | 2 | - | 2452 | 2556 | yes |  |
| 4x8 | anonymous hidden | full+data | yes | 3406 | 18 | 2 | - | 2616 | 2832 | yes |  |
| 4x8 | anonymous hidden | full+reply | yes | 3791 | 18 | 2 | - | 3001 | 3231 | yes |  |
| 4x8 | anonymous hidden | full+2tree | yes | 3148 | 19 | 2 | - | 2325 | 2556 | yes |  |
| 4x8 | anonymous public owners | min | yes | 2580 | 12 | 1 | - | 2065 | 2306 | yes |  |
| 4x8 | anonymous public owners | full | yes | 3239 | 22 | 3 | 4 | 2256 | 2455 | yes |  |
| 4x8 | anonymous public owners | full+spl | yes | 3607 | 29 | 3 | - | 2388 | 2459 | yes |  |
| 4x8 | anonymous public owners | full+data | yes | 3535 | 22 | 3 | - | 2552 | 2735 | yes |  |
| 4x8 | anonymous public owners | full+reply | yes | 3920 | 22 | 3 | - | 2937 | 3134 | yes |  |
| 4x8 | anonymous public owners | full+2tree | yes | 3277 | 23 | 3 | - | 2261 | 2459 | yes |  |
| 4x8 | confidential entry | min | yes | 3068 | 12 | 1 | - | 2553 | 2786 | yes |  |
| 4x8 | confidential entry | full | yes | 3727 | 22 | 3 | 4 | 2744 | 2935 | yes |  |
| 4x8 | confidential entry | full+spl | yes | 4095 | 29 | 3 | - | 2876 | 2939 | yes |  |
| 4x8 | confidential entry | full+data | yes | 4023 | 22 | 3 | - | 3040 | 3215 | yes |  |
| 4x8 | confidential entry | full+reply | yes | 3727 | 22 | 3 | - | 2744 | 2935 | yes |  |
| 4x8 | confidential entry | full+2tree | yes | 3765 | 23 | 3 | - | 2749 | 2939 | yes |  |
| 4x8 | anonymous entry | min | yes | 3227 | 12 | 1 | - | 2713 | 2955 | yes |  |
| 4x8 | anonymous entry | full | yes | 3694 | 18 | 2 | 4 | 2904 | 3104 | yes |  |
| 4x8 | anonymous entry | full+spl | yes | 4062 | 25 | 2 | - | 3036 | 3108 | yes |  |
| 4x8 | anonymous entry | full+data | yes | 3990 | 18 | 2 | - | 3200 | 3384 | yes |  |
| 4x8 | anonymous entry | full+reply | yes | 4375 | 18 | 2 | - | 3585 | 3783 | no | 279 B |
| 4x8 | anonymous entry | full+2tree | yes | 3732 | 19 | 2 | - | 2909 | 3108 | yes |  |
| 4x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x2 | confidential signer | min | yes | 1519 | 13 | 1 | - | 971 | 989 | yes |  |
| 5x2 | confidential signer | full | yes | 2178 | 23 | 3 | 5 | 1162 | 1138 | yes |  |
| 5x2 | confidential signer | full+spl | yes | 2546 | 30 | 3 | - | 1294 | 1142 | yes |  |
| 5x2 | confidential signer | full+data | yes | 2252 | 23 | 3 | - | 1236 | 1208 | yes |  |
| 5x2 | confidential signer | full+reply | yes | 2178 | 23 | 3 | - | 1162 | 1138 | yes |  |
| 5x2 | confidential signer | full+2tree | yes | 2216 | 24 | 3 | - | 1167 | 1142 | yes |  |
| 5x2 | confidential in-proof | min | yes | 1614 | 13 | 1 | - | 1067 | 1086 | yes |  |
| 5x2 | confidential in-proof | full | yes | 2177 | 22 | 2 | 5 | 1258 | 1235 | yes |  |
| 5x2 | confidential in-proof | full+spl | yes | 2545 | 29 | 2 | - | 1390 | 1239 | yes |  |
| 5x2 | confidential in-proof | full+data | yes | 2251 | 22 | 2 | - | 1332 | 1305 | yes |  |
| 5x2 | confidential in-proof | full+reply | yes | 2177 | 22 | 2 | - | 1258 | 1235 | yes |  |
| 5x2 | confidential in-proof | full+2tree | yes | 2215 | 23 | 2 | - | 1263 | 1239 | yes |  |
| 5x2 | anonymous hidden | min | yes | 1646 | 13 | 1 | - | 1099 | 1158 | yes |  |
| 5x2 | anonymous hidden | full | yes | 2113 | 19 | 2 | 5 | 1290 | 1307 | yes |  |
| 5x2 | anonymous hidden | full+spl | yes | 2481 | 26 | 2 | - | 1422 | 1311 | yes |  |
| 5x2 | anonymous hidden | full+data | yes | 2187 | 19 | 2 | - | 1364 | 1377 | yes |  |
| 5x2 | anonymous hidden | full+reply | yes | 2212 | 19 | 2 | - | 1389 | 1404 | yes |  |
| 5x2 | anonymous hidden | full+2tree | yes | 2151 | 20 | 2 | - | 1295 | 1311 | yes |  |
| 5x2 | anonymous public owners | min | yes | 1583 | 13 | 1 | - | 1035 | 1061 | yes |  |
| 5x2 | anonymous public owners | full | yes | 2242 | 23 | 3 | 5 | 1226 | 1210 | yes |  |
| 5x2 | anonymous public owners | full+spl | yes | 2610 | 30 | 3 | - | 1358 | 1214 | yes |  |
| 5x2 | anonymous public owners | full+data | yes | 2316 | 23 | 3 | - | 1300 | 1280 | yes |  |
| 5x2 | anonymous public owners | full+reply | yes | 2341 | 23 | 3 | - | 1325 | 1307 | yes |  |
| 5x2 | anonymous public owners | full+2tree | yes | 2280 | 24 | 3 | - | 1231 | 1214 | yes |  |
| 5x2 | confidential entry | min | yes | 2207 | 13 | 1 | - | 1659 | 1677 | yes |  |
| 5x2 | confidential entry | full | yes | 2866 | 23 | 3 | 5 | 1850 | 1826 | yes |  |
| 5x2 | confidential entry | full+spl | yes | 3234 | 30 | 3 | - | 1982 | 1830 | yes |  |
| 5x2 | confidential entry | full+data | yes | 2940 | 23 | 3 | - | 1924 | 1896 | yes |  |
| 5x2 | confidential entry | full+reply | yes | 2866 | 23 | 3 | - | 1850 | 1826 | yes |  |
| 5x2 | confidential entry | full+2tree | yes | 2904 | 24 | 3 | - | 1855 | 1830 | yes |  |
| 5x2 | anonymous entry | min | yes | 2366 | 13 | 1 | - | 1819 | 1846 | yes |  |
| 5x2 | anonymous entry | full | yes | 2833 | 19 | 2 | 5 | 2010 | 1995 | yes |  |
| 5x2 | anonymous entry | full+spl | yes | 3201 | 26 | 2 | - | 2142 | 1999 | yes |  |
| 5x2 | anonymous entry | full+data | yes | 2907 | 19 | 2 | - | 2084 | 2065 | yes |  |
| 5x2 | anonymous entry | full+reply | yes | 2932 | 19 | 2 | - | 2109 | 2092 | yes |  |
| 5x2 | anonymous entry | full+2tree | yes | 2871 | 20 | 2 | - | 2015 | 1999 | yes |  |
| 5x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x4 | confidential signer | min | yes | 1873 | 13 | 1 | - | 1325 | 1415 | yes |  |
| 5x4 | confidential signer | full | yes | 2532 | 23 | 3 | 5 | 1516 | 1564 | yes |  |
| 5x4 | confidential signer | full+spl | yes | 2900 | 30 | 3 | - | 1648 | 1568 | yes |  |
| 5x4 | confidential signer | full+data | yes | 2680 | 23 | 3 | - | 1664 | 1704 | yes |  |
| 5x4 | confidential signer | full+reply | yes | 2532 | 23 | 3 | - | 1516 | 1564 | yes |  |
| 5x4 | confidential signer | full+2tree | yes | 2570 | 24 | 3 | - | 1521 | 1568 | yes |  |
| 5x4 | confidential in-proof | min | yes | 1968 | 13 | 1 | - | 1421 | 1512 | yes |  |
| 5x4 | confidential in-proof | full | yes | 2531 | 22 | 2 | 5 | 1612 | 1661 | yes |  |
| 5x4 | confidential in-proof | full+spl | yes | 2899 | 29 | 2 | - | 1744 | 1665 | yes |  |
| 5x4 | confidential in-proof | full+data | yes | 2679 | 22 | 2 | - | 1760 | 1801 | yes |  |
| 5x4 | confidential in-proof | full+reply | yes | 2531 | 22 | 2 | - | 1612 | 1661 | yes |  |
| 5x4 | confidential in-proof | full+2tree | yes | 2569 | 23 | 2 | - | 1617 | 1665 | yes |  |
| 5x4 | anonymous hidden | min | yes | 2000 | 13 | 1 | - | 1453 | 1584 | yes |  |
| 5x4 | anonymous hidden | full | yes | 2467 | 19 | 2 | 5 | 1644 | 1733 | yes |  |
| 5x4 | anonymous hidden | full+spl | yes | 2835 | 26 | 2 | - | 1776 | 1737 | yes |  |
| 5x4 | anonymous hidden | full+data | yes | 2615 | 19 | 2 | - | 1792 | 1873 | yes |  |
| 5x4 | anonymous hidden | full+reply | yes | 2760 | 19 | 2 | - | 1937 | 2024 | yes |  |
| 5x4 | anonymous hidden | full+2tree | yes | 2505 | 20 | 2 | - | 1649 | 1737 | yes |  |
| 5x4 | anonymous public owners | min | yes | 1937 | 13 | 1 | - | 1389 | 1487 | yes |  |
| 5x4 | anonymous public owners | full | yes | 2596 | 23 | 3 | 5 | 1580 | 1636 | yes |  |
| 5x4 | anonymous public owners | full+spl | yes | 2964 | 30 | 3 | - | 1712 | 1640 | yes |  |
| 5x4 | anonymous public owners | full+data | yes | 2744 | 23 | 3 | - | 1728 | 1776 | yes |  |
| 5x4 | anonymous public owners | full+reply | yes | 2889 | 23 | 3 | - | 1873 | 1927 | yes |  |
| 5x4 | anonymous public owners | full+2tree | yes | 2634 | 24 | 3 | - | 1585 | 1640 | yes |  |
| 5x4 | confidential entry | min | yes | 2561 | 13 | 1 | - | 2013 | 2103 | yes |  |
| 5x4 | confidential entry | full | yes | 3220 | 23 | 3 | 5 | 2204 | 2252 | yes |  |
| 5x4 | confidential entry | full+spl | yes | 3588 | 30 | 3 | - | 2336 | 2256 | yes |  |
| 5x4 | confidential entry | full+data | yes | 3368 | 23 | 3 | - | 2352 | 2392 | yes |  |
| 5x4 | confidential entry | full+reply | yes | 3220 | 23 | 3 | - | 2204 | 2252 | yes |  |
| 5x4 | confidential entry | full+2tree | yes | 3258 | 24 | 3 | - | 2209 | 2256 | yes |  |
| 5x4 | anonymous entry | min | yes | 2720 | 13 | 1 | - | 2173 | 2272 | yes |  |
| 5x4 | anonymous entry | full | yes | 3187 | 19 | 2 | 5 | 2364 | 2421 | yes |  |
| 5x4 | anonymous entry | full+spl | yes | 3555 | 26 | 2 | - | 2496 | 2425 | yes |  |
| 5x4 | anonymous entry | full+data | yes | 3335 | 19 | 2 | - | 2512 | 2561 | yes |  |
| 5x4 | anonymous entry | full+reply | yes | 3480 | 19 | 2 | - | 2657 | 2712 | yes |  |
| 5x4 | anonymous entry | full+2tree | yes | 3225 | 20 | 2 | - | 2369 | 2425 | yes |  |
| 5x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x16 | confidential signer | min | yes | 3933 | 12 | 1 | - | 3418 | 3938 | yes |  |
| 4x16 | confidential signer | full | yes | 4592 | 22 | 3 | none | 3609 | 4087 | no | 496 B |
| 4x16 | confidential signer | full+spl | yes | 4960 | 29 | 3 | - | 3741 | 4091 | no | 864 B |
| 4x16 | confidential signer | full+data | yes | 5184 | 22 | 3 | - | 4201 | 4647 | no | 1088 B |
| 4x16 | confidential signer | full+reply | yes | 4592 | 22 | 3 | - | 3609 | 4087 | no | 496 B |
| 4x16 | confidential signer | full+2tree | yes | 4630 | 23 | 3 | - | 3614 | 4091 | no | 534 B |
| 4x16 | confidential in-proof | min | yes | 4028 | 12 | 1 | - | 3514 | 4035 | yes |  |
| 4x16 | confidential in-proof | full | yes | 4591 | 21 | 2 | none | 3705 | 4184 | no | 495 B |
| 4x16 | confidential in-proof | full+spl | yes | 4959 | 28 | 2 | - | 3837 | 4188 | no | 863 B |
| 4x16 | confidential in-proof | full+data | yes | 5183 | 21 | 2 | - | 4297 | 4744 | no | 1087 B |
| 4x16 | confidential in-proof | full+reply | yes | 4591 | 21 | 2 | - | 3705 | 4184 | no | 495 B |
| 4x16 | confidential in-proof | full+2tree | yes | 4629 | 22 | 2 | - | 3710 | 4188 | no | 533 B |
| 4x16 | anonymous hidden | min | yes | 4060 | 12 | 1 | - | 3546 | 4107 | yes |  |
| 4x16 | anonymous hidden | full | yes | 4527 | 18 | 2 | none | 3737 | 4256 | no | 431 B |
| 4x16 | anonymous hidden | full+spl | yes | 4895 | 25 | 2 | - | 3869 | 4260 | no | 799 B |
| 4x16 | anonymous hidden | full+data | yes | 5119 | 18 | 2 | - | 4329 | 4816 | no | 1023 B |
| 4x16 | anonymous hidden | full+reply | yes | 5984 | 18 | 2 | - | 5194 | 5711 | no | 1888 B |
| 4x16 | anonymous hidden | full+2tree | yes | 4565 | 19 | 2 | - | 3742 | 4260 | no | 469 B |
| 4x16 | anonymous public owners | min | yes | 3997 | 12 | 1 | - | 3482 | 4010 | yes |  |
| 4x16 | anonymous public owners | full | yes | 4656 | 22 | 3 | none | 3673 | 4159 | no | 560 B |
| 4x16 | anonymous public owners | full+spl | yes | 5024 | 29 | 3 | - | 3805 | 4163 | no | 928 B |
| 4x16 | anonymous public owners | full+data | yes | 5248 | 22 | 3 | - | 4265 | 4719 | no | 1152 B |
| 4x16 | anonymous public owners | full+reply | yes | 6113 | 22 | 3 | - | 5130 | 5614 | no | 2017 B |
| 4x16 | anonymous public owners | full+2tree | yes | 4694 | 23 | 3 | - | 3678 | 4163 | no | 598 B |
| 4x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x8 | confidential signer | min | yes | 2581 | 13 | 1 | - | 2033 | 2267 | yes |  |
| 5x8 | confidential signer | full | yes | 3240 | 23 | 3 | 5 | 2224 | 2416 | yes |  |
| 5x8 | confidential signer | full+spl | yes | 3608 | 30 | 3 | - | 2356 | 2420 | yes |  |
| 5x8 | confidential signer | full+data | yes | 3536 | 23 | 3 | - | 2520 | 2696 | yes |  |
| 5x8 | confidential signer | full+reply | yes | 3240 | 23 | 3 | - | 2224 | 2416 | yes |  |
| 5x8 | confidential signer | full+2tree | yes | 3278 | 24 | 3 | - | 2229 | 2420 | yes |  |
| 5x8 | confidential in-proof | min | yes | 2676 | 13 | 1 | - | 2129 | 2364 | yes |  |
| 5x8 | confidential in-proof | full | yes | 3239 | 22 | 2 | 5 | 2320 | 2513 | yes |  |
| 5x8 | confidential in-proof | full+spl | yes | 3607 | 29 | 2 | - | 2452 | 2517 | yes |  |
| 5x8 | confidential in-proof | full+data | yes | 3535 | 22 | 2 | - | 2616 | 2793 | yes |  |
| 5x8 | confidential in-proof | full+reply | yes | 3239 | 22 | 2 | - | 2320 | 2513 | yes |  |
| 5x8 | confidential in-proof | full+2tree | yes | 3277 | 23 | 2 | - | 2325 | 2517 | yes |  |
| 5x8 | anonymous hidden | min | yes | 2708 | 13 | 1 | - | 2161 | 2436 | yes |  |
| 5x8 | anonymous hidden | full | yes | 3175 | 19 | 2 | 5 | 2352 | 2585 | yes |  |
| 5x8 | anonymous hidden | full+spl | yes | 3543 | 26 | 2 | - | 2484 | 2589 | yes |  |
| 5x8 | anonymous hidden | full+data | yes | 3471 | 19 | 2 | - | 2648 | 2865 | yes |  |
| 5x8 | anonymous hidden | full+reply | yes | 3856 | 19 | 2 | - | 3033 | 3264 | yes |  |
| 5x8 | anonymous hidden | full+2tree | yes | 3213 | 20 | 2 | - | 2357 | 2589 | yes |  |
| 5x8 | anonymous public owners | min | yes | 2645 | 13 | 1 | - | 2097 | 2339 | yes |  |
| 5x8 | anonymous public owners | full | yes | 3304 | 23 | 3 | 5 | 2288 | 2488 | yes |  |
| 5x8 | anonymous public owners | full+spl | yes | 3672 | 30 | 3 | - | 2420 | 2492 | yes |  |
| 5x8 | anonymous public owners | full+data | yes | 3600 | 23 | 3 | - | 2584 | 2768 | yes |  |
| 5x8 | anonymous public owners | full+reply | yes | 3985 | 23 | 3 | - | 2969 | 3167 | yes |  |
| 5x8 | anonymous public owners | full+2tree | yes | 3342 | 24 | 3 | - | 2293 | 2492 | yes |  |
| 5x8 | confidential entry | min | yes | 3269 | 13 | 1 | - | 2721 | 2955 | yes |  |
| 5x8 | confidential entry | full | yes | 3928 | 23 | 3 | 2 | 2912 | 3104 | yes |  |
| 5x8 | confidential entry | full+spl | yes | 4296 | 30 | 3 | - | 3044 | 3108 | no | 200 B |
| 5x8 | confidential entry | full+data | yes | 4224 | 23 | 3 | - | 3208 | 3384 | no | 128 B |
| 5x8 | confidential entry | full+reply | yes | 3928 | 23 | 3 | - | 2912 | 3104 | yes |  |
| 5x8 | confidential entry | full+2tree | yes | 3966 | 24 | 3 | - | 2917 | 3108 | yes |  |
| 5x8 | anonymous entry | min | yes | 3428 | 13 | 1 | - | 2881 | 3124 | yes |  |
| 5x8 | anonymous entry | full | yes | 3895 | 19 | 2 | 2 | 3072 | 3273 | yes |  |
| 5x8 | anonymous entry | full+spl | yes | 4263 | 26 | 2 | - | 3204 | 3277 | no | 167 B |
| 5x8 | anonymous entry | full+data | yes | 4191 | 19 | 2 | - | 3368 | 3553 | no | 95 B |
| 5x8 | anonymous entry | full+reply | yes | 4576 | 19 | 2 | - | 3753 | 3952 | no | 480 B |
| 5x8 | anonymous entry | full+2tree | yes | 3933 | 20 | 2 | - | 3077 | 3277 | yes |  |
| 5x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x2 | confidential signer | min | yes | 1584 | 14 | 1 | - | 1003 | 1022 | yes |  |
| 6x2 | confidential signer | full | yes | 2243 | 24 | 3 | 6 | 1194 | 1171 | yes |  |
| 6x2 | confidential signer | full+spl | yes | 2611 | 31 | 3 | - | 1326 | 1175 | yes |  |
| 6x2 | confidential signer | full+data | yes | 2317 | 24 | 3 | - | 1268 | 1241 | yes |  |
| 6x2 | confidential signer | full+reply | yes | 2243 | 24 | 3 | - | 1194 | 1171 | yes |  |
| 6x2 | confidential signer | full+2tree | yes | 2281 | 25 | 3 | - | 1199 | 1175 | yes |  |
| 6x2 | confidential in-proof | min | yes | 1679 | 14 | 1 | - | 1099 | 1119 | yes |  |
| 6x2 | confidential in-proof | full | yes | 2242 | 23 | 2 | 6 | 1290 | 1268 | yes |  |
| 6x2 | confidential in-proof | full+spl | yes | 2610 | 30 | 2 | - | 1422 | 1272 | yes |  |
| 6x2 | confidential in-proof | full+data | yes | 2316 | 23 | 2 | - | 1364 | 1338 | yes |  |
| 6x2 | confidential in-proof | full+reply | yes | 2242 | 23 | 2 | - | 1290 | 1268 | yes |  |
| 6x2 | confidential in-proof | full+2tree | yes | 2280 | 24 | 2 | - | 1295 | 1272 | yes |  |
| 6x2 | anonymous hidden | min | yes | 1711 | 14 | 1 | - | 1131 | 1191 | yes |  |
| 6x2 | anonymous hidden | full | yes | 2178 | 20 | 2 | 6 | 1322 | 1340 | yes |  |
| 6x2 | anonymous hidden | full+spl | yes | 2546 | 27 | 2 | - | 1454 | 1344 | yes |  |
| 6x2 | anonymous hidden | full+data | yes | 2252 | 20 | 2 | - | 1396 | 1410 | yes |  |
| 6x2 | anonymous hidden | full+reply | yes | 2277 | 20 | 2 | - | 1421 | 1437 | yes |  |
| 6x2 | anonymous hidden | full+2tree | yes | 2216 | 21 | 2 | - | 1327 | 1344 | yes |  |
| 6x2 | anonymous public owners | min | yes | 1648 | 14 | 1 | - | 1067 | 1094 | yes |  |
| 6x2 | anonymous public owners | full | yes | 2307 | 24 | 3 | 6 | 1258 | 1243 | yes |  |
| 6x2 | anonymous public owners | full+spl | yes | 2675 | 31 | 3 | - | 1390 | 1247 | yes |  |
| 6x2 | anonymous public owners | full+data | yes | 2381 | 24 | 3 | - | 1332 | 1313 | yes |  |
| 6x2 | anonymous public owners | full+reply | yes | 2406 | 24 | 3 | - | 1357 | 1340 | yes |  |
| 6x2 | anonymous public owners | full+2tree | yes | 2345 | 25 | 3 | - | 1263 | 1247 | yes |  |
| 6x2 | confidential entry | min | yes | 2408 | 14 | 1 | - | 1827 | 1846 | yes |  |
| 6x2 | confidential entry | full | yes | 3067 | 24 | 3 | 6 | 2018 | 1995 | yes |  |
| 6x2 | confidential entry | full+spl | yes | 3435 | 31 | 3 | - | 2150 | 1999 | yes |  |
| 6x2 | confidential entry | full+data | yes | 3141 | 24 | 3 | - | 2092 | 2065 | yes |  |
| 6x2 | confidential entry | full+reply | yes | 3067 | 24 | 3 | - | 2018 | 1995 | yes |  |
| 6x2 | confidential entry | full+2tree | yes | 3105 | 25 | 3 | - | 2023 | 1999 | yes |  |
| 6x2 | anonymous entry | min | yes | 2567 | 14 | 1 | - | 1987 | 2015 | yes |  |
| 6x2 | anonymous entry | full | yes | 3034 | 20 | 2 | 6 | 2178 | 2164 | yes |  |
| 6x2 | anonymous entry | full+spl | yes | 3402 | 27 | 2 | - | 2310 | 2168 | yes |  |
| 6x2 | anonymous entry | full+data | yes | 3108 | 20 | 2 | - | 2252 | 2234 | yes |  |
| 6x2 | anonymous entry | full+reply | yes | 3133 | 20 | 2 | - | 2277 | 2261 | yes |  |
| 6x2 | anonymous entry | full+2tree | yes | 3072 | 21 | 2 | - | 2183 | 2168 | yes |  |
| 6x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x4 | confidential signer | min | yes | 1938 | 14 | 1 | - | 1357 | 1448 | yes |  |
| 6x4 | confidential signer | full | yes | 2597 | 24 | 3 | 6 | 1548 | 1597 | yes |  |
| 6x4 | confidential signer | full+spl | yes | 2965 | 31 | 3 | - | 1680 | 1601 | yes |  |
| 6x4 | confidential signer | full+data | yes | 2745 | 24 | 3 | - | 1696 | 1737 | yes |  |
| 6x4 | confidential signer | full+reply | yes | 2597 | 24 | 3 | - | 1548 | 1597 | yes |  |
| 6x4 | confidential signer | full+2tree | yes | 2635 | 25 | 3 | - | 1553 | 1601 | yes |  |
| 6x4 | confidential in-proof | min | yes | 2033 | 14 | 1 | - | 1453 | 1545 | yes |  |
| 6x4 | confidential in-proof | full | yes | 2596 | 23 | 2 | 6 | 1644 | 1694 | yes |  |
| 6x4 | confidential in-proof | full+spl | yes | 2964 | 30 | 2 | - | 1776 | 1698 | yes |  |
| 6x4 | confidential in-proof | full+data | yes | 2744 | 23 | 2 | - | 1792 | 1834 | yes |  |
| 6x4 | confidential in-proof | full+reply | yes | 2596 | 23 | 2 | - | 1644 | 1694 | yes |  |
| 6x4 | confidential in-proof | full+2tree | yes | 2634 | 24 | 2 | - | 1649 | 1698 | yes |  |
| 6x4 | anonymous hidden | min | yes | 2065 | 14 | 1 | - | 1485 | 1617 | yes |  |
| 6x4 | anonymous hidden | full | yes | 2532 | 20 | 2 | 6 | 1676 | 1766 | yes |  |
| 6x4 | anonymous hidden | full+spl | yes | 2900 | 27 | 2 | - | 1808 | 1770 | yes |  |
| 6x4 | anonymous hidden | full+data | yes | 2680 | 20 | 2 | - | 1824 | 1906 | yes |  |
| 6x4 | anonymous hidden | full+reply | yes | 2825 | 20 | 2 | - | 1969 | 2057 | yes |  |
| 6x4 | anonymous hidden | full+2tree | yes | 2570 | 21 | 2 | - | 1681 | 1770 | yes |  |
| 6x4 | anonymous public owners | min | yes | 2002 | 14 | 1 | - | 1421 | 1520 | yes |  |
| 6x4 | anonymous public owners | full | yes | 2661 | 24 | 3 | 6 | 1612 | 1669 | yes |  |
| 6x4 | anonymous public owners | full+spl | yes | 3029 | 31 | 3 | - | 1744 | 1673 | yes |  |
| 6x4 | anonymous public owners | full+data | yes | 2809 | 24 | 3 | - | 1760 | 1809 | yes |  |
| 6x4 | anonymous public owners | full+reply | yes | 2954 | 24 | 3 | - | 1905 | 1960 | yes |  |
| 6x4 | anonymous public owners | full+2tree | yes | 2699 | 25 | 3 | - | 1617 | 1673 | yes |  |
| 6x4 | confidential entry | min | yes | 2762 | 14 | 1 | - | 2181 | 2272 | yes |  |
| 6x4 | confidential entry | full | yes | 3421 | 24 | 3 | 6 | 2372 | 2421 | yes |  |
| 6x4 | confidential entry | full+spl | yes | 3789 | 31 | 3 | - | 2504 | 2425 | yes |  |
| 6x4 | confidential entry | full+data | yes | 3569 | 24 | 3 | - | 2520 | 2561 | yes |  |
| 6x4 | confidential entry | full+reply | yes | 3421 | 24 | 3 | - | 2372 | 2421 | yes |  |
| 6x4 | confidential entry | full+2tree | yes | 3459 | 25 | 3 | - | 2377 | 2425 | yes |  |
| 6x4 | anonymous entry | min | yes | 2921 | 14 | 1 | - | 2341 | 2441 | yes |  |
| 6x4 | anonymous entry | full | yes | 3388 | 20 | 2 | 6 | 2532 | 2590 | yes |  |
| 6x4 | anonymous entry | full+spl | yes | 3756 | 27 | 2 | - | 2664 | 2594 | yes |  |
| 6x4 | anonymous entry | full+data | yes | 3536 | 20 | 2 | - | 2680 | 2730 | yes |  |
| 6x4 | anonymous entry | full+reply | yes | 3681 | 20 | 2 | - | 2825 | 2881 | yes |  |
| 6x4 | anonymous entry | full+2tree | yes | 3426 | 21 | 2 | - | 2537 | 2594 | yes |  |
| 6x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x16 | confidential signer | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x8 | confidential signer | min | yes | 2646 | 14 | 1 | - | 2065 | 2300 | yes |  |
| 6x8 | confidential signer | full | yes | 3305 | 24 | 3 | 6 | 2256 | 2449 | yes |  |
| 6x8 | confidential signer | full+spl | yes | 3673 | 31 | 3 | - | 2388 | 2453 | yes |  |
| 6x8 | confidential signer | full+data | yes | 3601 | 24 | 3 | - | 2552 | 2729 | yes |  |
| 6x8 | confidential signer | full+reply | yes | 3305 | 24 | 3 | - | 2256 | 2449 | yes |  |
| 6x8 | confidential signer | full+2tree | yes | 3343 | 25 | 3 | - | 2261 | 2453 | yes |  |
| 6x8 | confidential in-proof | min | yes | 2741 | 14 | 1 | - | 2161 | 2397 | yes |  |
| 6x8 | confidential in-proof | full | yes | 3304 | 23 | 2 | 6 | 2352 | 2546 | yes |  |
| 6x8 | confidential in-proof | full+spl | yes | 3672 | 30 | 2 | - | 2484 | 2550 | yes |  |
| 6x8 | confidential in-proof | full+data | yes | 3600 | 23 | 2 | - | 2648 | 2826 | yes |  |
| 6x8 | confidential in-proof | full+reply | yes | 3304 | 23 | 2 | - | 2352 | 2546 | yes |  |
| 6x8 | confidential in-proof | full+2tree | yes | 3342 | 24 | 2 | - | 2357 | 2550 | yes |  |
| 6x8 | anonymous hidden | min | yes | 2773 | 14 | 1 | - | 2193 | 2469 | yes |  |
| 6x8 | anonymous hidden | full | yes | 3240 | 20 | 2 | 6 | 2384 | 2618 | yes |  |
| 6x8 | anonymous hidden | full+spl | yes | 3608 | 27 | 2 | - | 2516 | 2622 | yes |  |
| 6x8 | anonymous hidden | full+data | yes | 3536 | 20 | 2 | - | 2680 | 2898 | yes |  |
| 6x8 | anonymous hidden | full+reply | yes | 3921 | 20 | 2 | - | 3065 | 3297 | yes |  |
| 6x8 | anonymous hidden | full+2tree | yes | 3278 | 21 | 2 | - | 2389 | 2622 | yes |  |
| 6x8 | anonymous public owners | min | yes | 2710 | 14 | 1 | - | 2129 | 2372 | yes |  |
| 6x8 | anonymous public owners | full | yes | 3369 | 24 | 3 | 6 | 2320 | 2521 | yes |  |
| 6x8 | anonymous public owners | full+spl | yes | 3737 | 31 | 3 | - | 2452 | 2525 | yes |  |
| 6x8 | anonymous public owners | full+data | yes | 3665 | 24 | 3 | - | 2616 | 2801 | yes |  |
| 6x8 | anonymous public owners | full+reply | yes | 4050 | 24 | 3 | - | 3001 | 3200 | yes |  |
| 6x8 | anonymous public owners | full+2tree | yes | 3407 | 25 | 3 | - | 2325 | 2525 | yes |  |
| 6x8 | confidential entry | min | yes | 3470 | 14 | 1 | - | 2889 | 3124 | yes |  |
| 6x8 | confidential entry | full | yes | 4129 | 24 | 3 | none | 3080 | 3273 | no | 33 B |
| 6x8 | confidential entry | full+spl | yes | 4497 | 31 | 3 | - | 3212 | 3277 | no | 401 B |
| 6x8 | confidential entry | full+data | yes | 4425 | 24 | 3 | - | 3376 | 3553 | no | 329 B |
| 6x8 | confidential entry | full+reply | yes | 4129 | 24 | 3 | - | 3080 | 3273 | no | 33 B |
| 6x8 | confidential entry | full+2tree | yes | 4167 | 25 | 3 | - | 3085 | 3277 | no | 71 B |
| 6x8 | anonymous entry | min | yes | 3629 | 14 | 1 | - | 3049 | 3293 | yes |  |
| 6x8 | anonymous entry | full | yes | 4096 | 20 | 2 | 0 | 3240 | 3442 | yes |  |
| 6x8 | anonymous entry | full+spl | yes | 4464 | 27 | 2 | - | 3372 | 3446 | no | 368 B |
| 6x8 | anonymous entry | full+data | yes | 4392 | 20 | 2 | - | 3536 | 3722 | no | 296 B |
| 6x8 | anonymous entry | full+reply | yes | 4777 | 20 | 2 | - | 3921 | 4121 | no | 681 B |
| 6x8 | anonymous entry | full+2tree | yes | 4134 | 21 | 2 | - | 3245 | 3446 | no | 38 B |
| 6x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x2 | confidential signer | min | yes | 1714 | 16 | 1 | - | 1067 | 1088 | yes |  |
| 8x2 | confidential signer | full | yes | 2373 | 26 | 3 | 8 | 1258 | 1237 | yes |  |
| 8x2 | confidential signer | full+spl | yes | 2741 | 33 | 3 | - | 1390 | 1241 | yes |  |
| 8x2 | confidential signer | full+data | yes | 2447 | 26 | 3 | - | 1332 | 1307 | yes |  |
| 8x2 | confidential signer | full+reply | yes | 2373 | 26 | 3 | - | 1258 | 1237 | yes |  |
| 8x2 | confidential signer | full+2tree | yes | 2411 | 27 | 3 | - | 1263 | 1241 | yes |  |
| 8x2 | confidential in-proof | min | yes | 1809 | 16 | 1 | - | 1163 | 1185 | yes |  |
| 8x2 | confidential in-proof | full | yes | 2372 | 25 | 2 | 8 | 1354 | 1334 | yes |  |
| 8x2 | confidential in-proof | full+spl | yes | 2740 | 32 | 2 | - | 1486 | 1338 | yes |  |
| 8x2 | confidential in-proof | full+data | yes | 2446 | 25 | 2 | - | 1428 | 1404 | yes |  |
| 8x2 | confidential in-proof | full+reply | yes | 2372 | 25 | 2 | - | 1354 | 1334 | yes |  |
| 8x2 | confidential in-proof | full+2tree | yes | 2410 | 26 | 2 | - | 1359 | 1338 | yes |  |
| 8x2 | anonymous hidden | min | yes | 1841 | 16 | 1 | - | 1195 | 1257 | yes |  |
| 8x2 | anonymous hidden | full | yes | 2308 | 22 | 2 | 8 | 1386 | 1406 | yes |  |
| 8x2 | anonymous hidden | full+spl | yes | 2676 | 29 | 2 | - | 1518 | 1410 | yes |  |
| 8x2 | anonymous hidden | full+data | yes | 2382 | 22 | 2 | - | 1460 | 1476 | yes |  |
| 8x2 | anonymous hidden | full+reply | yes | 2407 | 22 | 2 | - | 1485 | 1503 | yes |  |
| 8x2 | anonymous hidden | full+2tree | yes | 2346 | 23 | 2 | - | 1391 | 1410 | yes |  |
| 8x2 | anonymous public owners | min | yes | 1778 | 16 | 1 | - | 1131 | 1160 | yes |  |
| 8x2 | anonymous public owners | full | yes | 2437 | 26 | 3 | 8 | 1322 | 1309 | yes |  |
| 8x2 | anonymous public owners | full+spl | yes | 2805 | 33 | 3 | - | 1454 | 1313 | yes |  |
| 8x2 | anonymous public owners | full+data | yes | 2511 | 26 | 3 | - | 1396 | 1379 | yes |  |
| 8x2 | anonymous public owners | full+reply | yes | 2536 | 26 | 3 | - | 1421 | 1406 | yes |  |
| 8x2 | anonymous public owners | full+2tree | yes | 2475 | 27 | 3 | - | 1327 | 1313 | yes |  |
| 8x2 | confidential entry | min | yes | 2810 | 16 | 1 | - | 2163 | 2184 | yes |  |
| 8x2 | confidential entry | full | yes | 3469 | 26 | 3 | 7 | 2354 | 2333 | yes |  |
| 8x2 | confidential entry | full+spl | yes | 3837 | 33 | 3 | - | 2486 | 2337 | yes |  |
| 8x2 | confidential entry | full+data | yes | 3543 | 26 | 3 | - | 2428 | 2403 | yes |  |
| 8x2 | confidential entry | full+reply | yes | 3469 | 26 | 3 | - | 2354 | 2333 | yes |  |
| 8x2 | confidential entry | full+2tree | yes | 3507 | 27 | 3 | - | 2359 | 2337 | yes |  |
| 8x2 | anonymous entry | min | yes | 2969 | 16 | 1 | - | 2323 | 2353 | yes |  |
| 8x2 | anonymous entry | full | yes | 3436 | 22 | 2 | 6 | 2514 | 2502 | yes |  |
| 8x2 | anonymous entry | full+spl | yes | 3804 | 29 | 2 | - | 2646 | 2506 | yes |  |
| 8x2 | anonymous entry | full+data | yes | 3510 | 22 | 2 | - | 2588 | 2572 | yes |  |
| 8x2 | anonymous entry | full+reply | yes | 3535 | 22 | 2 | - | 2613 | 2599 | yes |  |
| 8x2 | anonymous entry | full+2tree | yes | 3474 | 23 | 2 | - | 2519 | 2506 | yes |  |
| 8x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x4 | confidential signer | min | yes | 2068 | 16 | 1 | - | 1421 | 1514 | yes |  |
| 8x4 | confidential signer | full | yes | 2727 | 26 | 3 | 8 | 1612 | 1663 | yes |  |
| 8x4 | confidential signer | full+spl | yes | 3095 | 33 | 3 | - | 1744 | 1667 | yes |  |
| 8x4 | confidential signer | full+data | yes | 2875 | 26 | 3 | - | 1760 | 1803 | yes |  |
| 8x4 | confidential signer | full+reply | yes | 2727 | 26 | 3 | - | 1612 | 1663 | yes |  |
| 8x4 | confidential signer | full+2tree | yes | 2765 | 27 | 3 | - | 1617 | 1667 | yes |  |
| 8x4 | confidential in-proof | min | yes | 2163 | 16 | 1 | - | 1517 | 1611 | yes |  |
| 8x4 | confidential in-proof | full | yes | 2726 | 25 | 2 | 8 | 1708 | 1760 | yes |  |
| 8x4 | confidential in-proof | full+spl | yes | 3094 | 32 | 2 | - | 1840 | 1764 | yes |  |
| 8x4 | confidential in-proof | full+data | yes | 2874 | 25 | 2 | - | 1856 | 1900 | yes |  |
| 8x4 | confidential in-proof | full+reply | yes | 2726 | 25 | 2 | - | 1708 | 1760 | yes |  |
| 8x4 | confidential in-proof | full+2tree | yes | 2764 | 26 | 2 | - | 1713 | 1764 | yes |  |
| 8x4 | anonymous hidden | min | yes | 2195 | 16 | 1 | - | 1549 | 1683 | yes |  |
| 8x4 | anonymous hidden | full | yes | 2662 | 22 | 2 | 8 | 1740 | 1832 | yes |  |
| 8x4 | anonymous hidden | full+spl | yes | 3030 | 29 | 2 | - | 1872 | 1836 | yes |  |
| 8x4 | anonymous hidden | full+data | yes | 2810 | 22 | 2 | - | 1888 | 1972 | yes |  |
| 8x4 | anonymous hidden | full+reply | yes | 2955 | 22 | 2 | - | 2033 | 2123 | yes |  |
| 8x4 | anonymous hidden | full+2tree | yes | 2700 | 23 | 2 | - | 1745 | 1836 | yes |  |
| 8x4 | anonymous public owners | min | yes | 2132 | 16 | 1 | - | 1485 | 1586 | yes |  |
| 8x4 | anonymous public owners | full | yes | 2791 | 26 | 3 | 8 | 1676 | 1735 | yes |  |
| 8x4 | anonymous public owners | full+spl | yes | 3159 | 33 | 3 | - | 1808 | 1739 | yes |  |
| 8x4 | anonymous public owners | full+data | yes | 2939 | 26 | 3 | - | 1824 | 1875 | yes |  |
| 8x4 | anonymous public owners | full+reply | yes | 3084 | 26 | 3 | - | 1969 | 2026 | yes |  |
| 8x4 | anonymous public owners | full+2tree | yes | 2829 | 27 | 3 | - | 1681 | 1739 | yes |  |
| 8x4 | confidential entry | min | yes | 3164 | 16 | 1 | - | 2517 | 2610 | yes |  |
| 8x4 | confidential entry | full | yes | 3823 | 26 | 3 | 3 | 2708 | 2759 | yes |  |
| 8x4 | confidential entry | full+spl | yes | 4191 | 33 | 3 | - | 2840 | 2763 | no | 95 B |
| 8x4 | confidential entry | full+data | yes | 3971 | 26 | 3 | - | 2856 | 2899 | yes |  |
| 8x4 | confidential entry | full+reply | yes | 3823 | 26 | 3 | - | 2708 | 2759 | yes |  |
| 8x4 | confidential entry | full+2tree | yes | 3861 | 27 | 3 | - | 2713 | 2763 | yes |  |
| 8x4 | anonymous entry | min | yes | 3323 | 16 | 1 | - | 2677 | 2779 | yes |  |
| 8x4 | anonymous entry | full | yes | 3790 | 22 | 2 | 3 | 2868 | 2928 | yes |  |
| 8x4 | anonymous entry | full+spl | yes | 4158 | 29 | 2 | - | 3000 | 2932 | no | 62 B |
| 8x4 | anonymous entry | full+data | yes | 3938 | 22 | 2 | - | 3016 | 3068 | yes |  |
| 8x4 | anonymous entry | full+reply | yes | 4083 | 22 | 2 | - | 3161 | 3219 | yes |  |
| 8x4 | anonymous entry | full+2tree | yes | 3828 | 23 | 2 | - | 2873 | 2932 | yes |  |
| 8x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x8 | confidential signer | min | yes | 2776 | 16 | 1 | - | 2129 | 2366 | yes |  |
| 8x8 | confidential signer | full | yes | 3435 | 26 | 3 | 7 | 2320 | 2515 | yes |  |
| 8x8 | confidential signer | full+spl | yes | 3803 | 33 | 3 | - | 2452 | 2519 | yes |  |
| 8x8 | confidential signer | full+data | yes | 3731 | 26 | 3 | - | 2616 | 2795 | yes |  |
| 8x8 | confidential signer | full+reply | yes | 3435 | 26 | 3 | - | 2320 | 2515 | yes |  |
| 8x8 | confidential signer | full+2tree | yes | 3473 | 27 | 3 | - | 2325 | 2519 | yes |  |
| 8x8 | confidential in-proof | min | yes | 2871 | 16 | 1 | - | 2225 | 2463 | yes |  |
| 8x8 | confidential in-proof | full | yes | 3434 | 25 | 2 | 6 | 2416 | 2612 | yes |  |
| 8x8 | confidential in-proof | full+spl | yes | 3802 | 32 | 2 | - | 2548 | 2616 | yes |  |
| 8x8 | confidential in-proof | full+data | yes | 3730 | 25 | 2 | - | 2712 | 2892 | yes |  |
| 8x8 | confidential in-proof | full+reply | yes | 3434 | 25 | 2 | - | 2416 | 2612 | yes |  |
| 8x8 | confidential in-proof | full+2tree | yes | 3472 | 26 | 2 | - | 2421 | 2616 | yes |  |
| 8x8 | anonymous hidden | min | yes | 2903 | 16 | 1 | - | 2257 | 2535 | yes |  |
| 8x8 | anonymous hidden | full | yes | 3370 | 22 | 2 | 7 | 2448 | 2684 | yes |  |
| 8x8 | anonymous hidden | full+spl | yes | 3738 | 29 | 2 | - | 2580 | 2688 | yes |  |
| 8x8 | anonymous hidden | full+data | yes | 3666 | 22 | 2 | - | 2744 | 2964 | yes |  |
| 8x8 | anonymous hidden | full+reply | yes | 4051 | 22 | 2 | - | 3129 | 3363 | yes |  |
| 8x8 | anonymous hidden | full+2tree | yes | 3408 | 23 | 2 | - | 2453 | 2688 | yes |  |
| 8x8 | anonymous public owners | min | yes | 2840 | 16 | 1 | - | 2193 | 2438 | yes |  |
| 8x8 | anonymous public owners | full | yes | 3499 | 26 | 3 | 7 | 2384 | 2587 | yes |  |
| 8x8 | anonymous public owners | full+spl | yes | 3867 | 33 | 3 | - | 2516 | 2591 | yes |  |
| 8x8 | anonymous public owners | full+data | yes | 3795 | 26 | 3 | - | 2680 | 2867 | yes |  |
| 8x8 | anonymous public owners | full+reply | yes | 4180 | 26 | 3 | - | 3065 | 3266 | no | 84 B |
| 8x8 | anonymous public owners | full+2tree | yes | 3537 | 27 | 3 | - | 2389 | 2591 | yes |  |
| 8x8 | confidential entry | min | yes | 3872 | 16 | 1 | - | 3225 | 3462 | yes |  |
| 8x8 | confidential entry | full | yes | 4531 | 26 | 3 | none | 3416 | 3611 | no | 435 B |
| 8x8 | confidential entry | full+spl | yes | 4899 | 33 | 3 | - | 3548 | 3615 | no | 803 B |
| 8x8 | confidential entry | full+data | yes | 4827 | 26 | 3 | - | 3712 | 3891 | no | 731 B |
| 8x8 | confidential entry | full+reply | yes | 4531 | 26 | 3 | - | 3416 | 3611 | no | 435 B |
| 8x8 | confidential entry | full+2tree | yes | 4569 | 27 | 3 | - | 3421 | 3615 | no | 473 B |
| 8x8 | anonymous entry | min | yes | 4031 | 16 | 1 | - | 3385 | 3631 | yes |  |
| 8x8 | anonymous entry | full | yes | 4498 | 22 | 2 | none | 3576 | 3780 | no | 402 B |
| 8x8 | anonymous entry | full+spl | yes | 4866 | 29 | 2 | - | 3708 | 3784 | no | 770 B |
| 8x8 | anonymous entry | full+data | yes | 4794 | 22 | 2 | - | 3872 | 4060 | no | 698 B |
| 8x8 | anonymous entry | full+reply | yes | 5179 | 22 | 2 | - | 4257 | 4459 | no | 1083 B |
| 8x8 | anonymous entry | full+2tree | yes | 4536 | 23 | 2 | - | 3581 | 3784 | no | 440 B |
| 8x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x16 | confidential signer | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x2 | confidential signer | min | yes | 1974 | 20 | 1 | - | 1195 | 1220 | yes |  |
| 12x2 | confidential signer | full | yes | 2633 | 30 | 3 | 10 | 1386 | 1369 | yes |  |
| 12x2 | confidential signer | full+spl | yes | 3001 | 37 | 3 | - | 1518 | 1373 | yes |  |
| 12x2 | confidential signer | full+data | yes | 2707 | 30 | 3 | - | 1460 | 1439 | yes |  |
| 12x2 | confidential signer | full+reply | yes | 2633 | 30 | 3 | - | 1386 | 1369 | yes |  |
| 12x2 | confidential signer | full+2tree | yes | 2672 | 31 | 3 | - | 1392 | 1373 | yes |  |
| 12x2 | confidential in-proof | min | yes | 2069 | 20 | 1 | - | 1291 | 1317 | yes |  |
| 12x2 | confidential in-proof | full | yes | 2632 | 29 | 2 | 10 | 1482 | 1466 | yes |  |
| 12x2 | confidential in-proof | full+spl | yes | 3000 | 36 | 2 | - | 1614 | 1470 | yes |  |
| 12x2 | confidential in-proof | full+data | yes | 2706 | 29 | 2 | - | 1556 | 1536 | yes |  |
| 12x2 | confidential in-proof | full+reply | yes | 2632 | 29 | 2 | - | 1482 | 1466 | yes |  |
| 12x2 | confidential in-proof | full+2tree | yes | 2671 | 30 | 2 | - | 1488 | 1470 | yes |  |
| 12x2 | anonymous hidden | min | yes | 2101 | 20 | 1 | - | 1323 | 1389 | yes |  |
| 12x2 | anonymous hidden | full | yes | 2568 | 26 | 2 | 10 | 1514 | 1538 | yes |  |
| 12x2 | anonymous hidden | full+spl | yes | 2936 | 33 | 2 | - | 1646 | 1542 | yes |  |
| 12x2 | anonymous hidden | full+data | yes | 2642 | 26 | 2 | - | 1588 | 1608 | yes |  |
| 12x2 | anonymous hidden | full+reply | yes | 2667 | 26 | 2 | - | 1613 | 1635 | yes |  |
| 12x2 | anonymous hidden | full+2tree | yes | 2607 | 27 | 2 | - | 1520 | 1542 | yes |  |
| 12x2 | anonymous public owners | min | yes | 2038 | 20 | 1 | - | 1259 | 1292 | yes |  |
| 12x2 | anonymous public owners | full | yes | 2697 | 30 | 3 | 10 | 1450 | 1441 | yes |  |
| 12x2 | anonymous public owners | full+spl | yes | 3065 | 37 | 3 | - | 1582 | 1445 | yes |  |
| 12x2 | anonymous public owners | full+data | yes | 2771 | 30 | 3 | - | 1524 | 1511 | yes |  |
| 12x2 | anonymous public owners | full+reply | yes | 2796 | 30 | 3 | - | 1549 | 1538 | yes |  |
| 12x2 | anonymous public owners | full+2tree | yes | 2736 | 31 | 3 | - | 1456 | 1445 | yes |  |
| 12x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x4 | confidential signer | min | yes | 2328 | 20 | 1 | - | 1549 | 1646 | yes |  |
| 12x4 | confidential signer | full | yes | 2987 | 30 | 3 | 10 | 1740 | 1795 | yes |  |
| 12x4 | confidential signer | full+spl | yes | 3355 | 37 | 3 | - | 1872 | 1799 | yes |  |
| 12x4 | confidential signer | full+data | yes | 3135 | 30 | 3 | - | 1888 | 1935 | yes |  |
| 12x4 | confidential signer | full+reply | yes | 2987 | 30 | 3 | - | 1740 | 1795 | yes |  |
| 12x4 | confidential signer | full+2tree | yes | 3026 | 31 | 3 | - | 1746 | 1799 | yes |  |
| 12x4 | confidential in-proof | min | yes | 2423 | 20 | 1 | - | 1645 | 1743 | yes |  |
| 12x4 | confidential in-proof | full | yes | 2986 | 29 | 2 | 10 | 1836 | 1892 | yes |  |
| 12x4 | confidential in-proof | full+spl | yes | 3354 | 36 | 2 | - | 1968 | 1896 | yes |  |
| 12x4 | confidential in-proof | full+data | yes | 3134 | 29 | 2 | - | 1984 | 2032 | yes |  |
| 12x4 | confidential in-proof | full+reply | yes | 2986 | 29 | 2 | - | 1836 | 1892 | yes |  |
| 12x4 | confidential in-proof | full+2tree | yes | 3025 | 30 | 2 | - | 1842 | 1896 | yes |  |
| 12x4 | anonymous hidden | min | yes | 2455 | 20 | 1 | - | 1677 | 1815 | yes |  |
| 12x4 | anonymous hidden | full | yes | 2922 | 26 | 2 | 10 | 1868 | 1964 | yes |  |
| 12x4 | anonymous hidden | full+spl | yes | 3290 | 33 | 2 | - | 2000 | 1968 | yes |  |
| 12x4 | anonymous hidden | full+data | yes | 3070 | 26 | 2 | - | 2016 | 2104 | yes |  |
| 12x4 | anonymous hidden | full+reply | yes | 3215 | 26 | 2 | - | 2161 | 2255 | yes |  |
| 12x4 | anonymous hidden | full+2tree | yes | 2961 | 27 | 2 | - | 1874 | 1968 | yes |  |
| 12x4 | anonymous public owners | min | yes | 2392 | 20 | 1 | - | 1613 | 1718 | yes |  |
| 12x4 | anonymous public owners | full | yes | 3051 | 30 | 3 | 10 | 1804 | 1867 | yes |  |
| 12x4 | anonymous public owners | full+spl | yes | 3419 | 37 | 3 | - | 1936 | 1871 | yes |  |
| 12x4 | anonymous public owners | full+data | yes | 3199 | 30 | 3 | - | 1952 | 2007 | yes |  |
| 12x4 | anonymous public owners | full+reply | yes | 3344 | 30 | 3 | - | 2097 | 2158 | yes |  |
| 12x4 | anonymous public owners | full+2tree | yes | 3090 | 31 | 3 | - | 1810 | 1871 | yes |  |
| 12x4 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x8 | confidential signer | min | yes | 3036 | 20 | 1 | - | 2257 | 2498 | yes |  |
| 12x8 | confidential signer | full | yes | 3695 | 30 | 3 | 5 | 2448 | 2647 | yes |  |
| 12x8 | confidential signer | full+spl | yes | 4063 | 37 | 3 | - | 2580 | 2651 | yes |  |
| 12x8 | confidential signer | full+data | yes | 3991 | 30 | 3 | - | 2744 | 2927 | yes |  |
| 12x8 | confidential signer | full+reply | yes | 3695 | 30 | 3 | - | 2448 | 2647 | yes |  |
| 12x8 | confidential signer | full+2tree | yes | 3734 | 31 | 3 | - | 2454 | 2651 | yes |  |
| 12x8 | confidential in-proof | min | yes | 3131 | 20 | 1 | - | 2353 | 2595 | yes |  |
| 12x8 | confidential in-proof | full | yes | 3694 | 29 | 2 | 4 | 2544 | 2744 | yes |  |
| 12x8 | confidential in-proof | full+spl | yes | 4062 | 36 | 2 | - | 2676 | 2748 | yes |  |
| 12x8 | confidential in-proof | full+data | yes | 3990 | 29 | 2 | - | 2840 | 3024 | yes |  |
| 12x8 | confidential in-proof | full+reply | yes | 3694 | 29 | 2 | - | 2544 | 2744 | yes |  |
| 12x8 | confidential in-proof | full+2tree | yes | 3733 | 30 | 2 | - | 2550 | 2748 | yes |  |
| 12x8 | anonymous hidden | min | yes | 3163 | 20 | 1 | - | 2385 | 2667 | yes |  |
| 12x8 | anonymous hidden | full | yes | 3630 | 26 | 2 | 4 | 2576 | 2816 | yes |  |
| 12x8 | anonymous hidden | full+spl | yes | 3998 | 33 | 2 | - | 2708 | 2820 | yes |  |
| 12x8 | anonymous hidden | full+data | yes | 3926 | 26 | 2 | - | 2872 | 3096 | yes |  |
| 12x8 | anonymous hidden | full+reply | yes | 4311 | 26 | 2 | - | 3257 | 3495 | no | 215 B |
| 12x8 | anonymous hidden | full+2tree | yes | 3669 | 27 | 2 | - | 2582 | 2820 | yes |  |
| 12x8 | anonymous public owners | min | yes | 3100 | 20 | 1 | - | 2321 | 2570 | yes |  |
| 12x8 | anonymous public owners | full | yes | 3759 | 30 | 3 | 4 | 2512 | 2719 | yes |  |
| 12x8 | anonymous public owners | full+spl | yes | 4127 | 37 | 3 | - | 2644 | 2723 | no | 31 B |
| 12x8 | anonymous public owners | full+data | yes | 4055 | 30 | 3 | - | 2808 | 2999 | yes |  |
| 12x8 | anonymous public owners | full+reply | yes | 4440 | 30 | 3 | - | 3193 | 3398 | no | 344 B |
| 12x8 | anonymous public owners | full+2tree | yes | 3798 | 31 | 3 | - | 2518 | 2723 | yes |  |
| 12x8 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x2 | confidential signer | min | yes | 2234 | 24 | 1 | - | 1323 | 1352 | yes |  |
| 16x2 | confidential signer | full | yes | 2893 | 34 | 3 | 10 | 1514 | 1501 | yes |  |
| 16x2 | confidential signer | full+spl | yes | 3261 | 41 | 3 | - | 1646 | 1505 | yes |  |
| 16x2 | confidential signer | full+data | yes | 2967 | 34 | 3 | - | 1588 | 1571 | yes |  |
| 16x2 | confidential signer | full+reply | yes | 2893 | 34 | 3 | - | 1514 | 1501 | yes |  |
| 16x2 | confidential signer | full+2tree | yes | 2932 | 35 | 3 | - | 1520 | 1505 | yes |  |
| 16x2 | confidential in-proof | min | yes | 2329 | 24 | 1 | - | 1419 | 1449 | yes |  |
| 16x2 | confidential in-proof | full | yes | 2892 | 33 | 2 | 10 | 1610 | 1598 | yes |  |
| 16x2 | confidential in-proof | full+spl | yes | 3260 | 40 | 2 | - | 1742 | 1602 | yes |  |
| 16x2 | confidential in-proof | full+data | yes | 2966 | 33 | 2 | - | 1684 | 1668 | yes |  |
| 16x2 | confidential in-proof | full+reply | yes | 2892 | 33 | 2 | - | 1610 | 1598 | yes |  |
| 16x2 | confidential in-proof | full+2tree | yes | 2931 | 34 | 2 | - | 1616 | 1602 | yes |  |
| 16x2 | anonymous hidden | min | yes | 2361 | 24 | 1 | - | 1451 | 1521 | yes |  |
| 16x2 | anonymous hidden | full | yes | 2828 | 30 | 2 | 10 | 1642 | 1670 | yes |  |
| 16x2 | anonymous hidden | full+spl | yes | 3196 | 37 | 2 | - | 1774 | 1674 | yes |  |
| 16x2 | anonymous hidden | full+data | yes | 2902 | 30 | 2 | - | 1716 | 1740 | yes |  |
| 16x2 | anonymous hidden | full+reply | yes | 2927 | 30 | 2 | - | 1741 | 1767 | yes |  |
| 16x2 | anonymous hidden | full+2tree | yes | 2867 | 31 | 2 | - | 1648 | 1674 | yes |  |
| 16x2 | anonymous public owners | min | yes | 2298 | 24 | 1 | - | 1387 | 1424 | yes |  |
| 16x2 | anonymous public owners | full | yes | 2957 | 34 | 3 | 10 | 1578 | 1573 | yes |  |
| 16x2 | anonymous public owners | full+spl | yes | 3325 | 41 | 3 | - | 1710 | 1577 | yes |  |
| 16x2 | anonymous public owners | full+data | yes | 3031 | 34 | 3 | - | 1652 | 1643 | yes |  |
| 16x2 | anonymous public owners | full+reply | yes | 3056 | 34 | 3 | - | 1677 | 1670 | yes |  |
| 16x2 | anonymous public owners | full+2tree | yes | 2996 | 35 | 3 | - | 1584 | 1577 | yes |  |
| 16x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x4 | confidential signer | min | yes | 2588 | 24 | 1 | - | 1677 | 1778 | yes |  |
| 16x4 | confidential signer | full | yes | 3247 | 34 | 3 | 9 | 1868 | 1927 | yes |  |
| 16x4 | confidential signer | full+spl | yes | 3615 | 41 | 3 | - | 2000 | 1931 | yes |  |
| 16x4 | confidential signer | full+data | yes | 3395 | 34 | 3 | - | 2016 | 2067 | yes |  |
| 16x4 | confidential signer | full+reply | yes | 3247 | 34 | 3 | - | 1868 | 1927 | yes |  |
| 16x4 | confidential signer | full+2tree | yes | 3286 | 35 | 3 | - | 1874 | 1931 | yes |  |
| 16x4 | confidential in-proof | min | yes | 2683 | 24 | 1 | - | 1773 | 1875 | yes |  |
| 16x4 | confidential in-proof | full | yes | 3246 | 33 | 2 | 8 | 1964 | 2024 | yes |  |
| 16x4 | confidential in-proof | full+spl | yes | 3614 | 40 | 2 | - | 2096 | 2028 | yes |  |
| 16x4 | confidential in-proof | full+data | yes | 3394 | 33 | 2 | - | 2112 | 2164 | yes |  |
| 16x4 | confidential in-proof | full+reply | yes | 3246 | 33 | 2 | - | 1964 | 2024 | yes |  |
| 16x4 | confidential in-proof | full+2tree | yes | 3285 | 34 | 2 | - | 1970 | 2028 | yes |  |
| 16x4 | anonymous hidden | min | yes | 2715 | 24 | 1 | - | 1805 | 1947 | yes |  |
| 16x4 | anonymous hidden | full | yes | 3182 | 30 | 2 | 9 | 1996 | 2096 | yes |  |
| 16x4 | anonymous hidden | full+spl | yes | 3550 | 37 | 2 | - | 2128 | 2100 | yes |  |
| 16x4 | anonymous hidden | full+data | yes | 3330 | 30 | 2 | - | 2144 | 2236 | yes |  |
| 16x4 | anonymous hidden | full+reply | yes | 3475 | 30 | 2 | - | 2289 | 2387 | yes |  |
| 16x4 | anonymous hidden | full+2tree | yes | 3221 | 31 | 2 | - | 2002 | 2100 | yes |  |
| 16x4 | anonymous public owners | min | yes | 2652 | 24 | 1 | - | 1741 | 1850 | yes |  |
| 16x4 | anonymous public owners | full | yes | 3311 | 34 | 3 | 9 | 1932 | 1999 | yes |  |
| 16x4 | anonymous public owners | full+spl | yes | 3679 | 41 | 3 | - | 2064 | 2003 | yes |  |
| 16x4 | anonymous public owners | full+data | yes | 3459 | 34 | 3 | - | 2080 | 2139 | yes |  |
| 16x4 | anonymous public owners | full+reply | yes | 3604 | 34 | 3 | - | 2225 | 2290 | yes |  |
| 16x4 | anonymous public owners | full+2tree | yes | 3350 | 35 | 3 | - | 1938 | 2003 | yes |  |
| 16x4 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x8 | confidential signer | min | yes | 3296 | 24 | 1 | - | 2385 | 2630 | yes |  |
| 16x8 | confidential signer | full | yes | 3955 | 34 | 3 | 2 | 2576 | 2779 | yes |  |
| 16x8 | confidential signer | full+spl | yes | 4323 | 41 | 3 | - | 2708 | 2783 | no | 227 B |
| 16x8 | confidential signer | full+data | yes | 4251 | 34 | 3 | - | 2872 | 3059 | no | 155 B |
| 16x8 | confidential signer | full+reply | yes | 3955 | 34 | 3 | - | 2576 | 2779 | yes |  |
| 16x8 | confidential signer | full+2tree | yes | 3994 | 35 | 3 | - | 2582 | 2783 | yes |  |
| 16x8 | confidential in-proof | min | yes | 3391 | 24 | 1 | - | 2481 | 2727 | yes |  |
| 16x8 | confidential in-proof | full | yes | 3954 | 33 | 2 | 1 | 2672 | 2876 | yes |  |
| 16x8 | confidential in-proof | full+spl | yes | 4322 | 40 | 2 | - | 2804 | 2880 | no | 226 B |
| 16x8 | confidential in-proof | full+data | yes | 4250 | 33 | 2 | - | 2968 | 3156 | no | 154 B |
| 16x8 | confidential in-proof | full+reply | yes | 3954 | 33 | 2 | - | 2672 | 2876 | yes |  |
| 16x8 | confidential in-proof | full+2tree | yes | 3993 | 34 | 2 | - | 2678 | 2880 | yes |  |
| 16x8 | anonymous hidden | min | yes | 3423 | 24 | 1 | - | 2513 | 2799 | yes |  |
| 16x8 | anonymous hidden | full | yes | 3890 | 30 | 2 | 2 | 2704 | 2948 | yes |  |
| 16x8 | anonymous hidden | full+spl | yes | 4258 | 37 | 2 | - | 2836 | 2952 | no | 162 B |
| 16x8 | anonymous hidden | full+data | yes | 4186 | 30 | 2 | - | 3000 | 3228 | no | 90 B |
| 16x8 | anonymous hidden | full+reply | yes | 4571 | 30 | 2 | - | 3385 | 3627 | no | 475 B |
| 16x8 | anonymous hidden | full+2tree | yes | 3929 | 31 | 2 | - | 2710 | 2952 | yes |  |
| 16x8 | anonymous public owners | min | yes | 3360 | 24 | 1 | - | 2449 | 2702 | yes |  |
| 16x8 | anonymous public owners | full | yes | 4019 | 34 | 3 | 1 | 2640 | 2851 | yes |  |
| 16x8 | anonymous public owners | full+spl | yes | 4387 | 41 | 3 | - | 2772 | 2855 | no | 291 B |
| 16x8 | anonymous public owners | full+data | yes | 4315 | 34 | 3 | - | 2936 | 3131 | no | 219 B |
| 16x8 | anonymous public owners | full+reply | yes | 4700 | 34 | 3 | - | 3321 | 3530 | no | 604 B |
| 16x8 | anonymous public owners | full+2tree | yes | 4058 | 35 | 3 | - | 2646 | 2855 | yes |  |
| 16x8 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 24x2 | confidential signer | min | yes | 2754 | 32 | 1 | - | 1579 | 1616 | yes |  |
| 24x2 | confidential signer | full | yes | 3413 | 42 | 3 | 8 | 1770 | 1765 | yes |  |
| 24x2 | confidential signer | full+spl | yes | 3781 | 49 | 3 | - | 1902 | 1769 | yes |  |
| 24x2 | confidential signer | full+data | yes | 3487 | 42 | 3 | - | 1844 | 1835 | yes |  |
| 24x2 | confidential signer | full+reply | yes | 3413 | 42 | 3 | - | 1770 | 1765 | yes |  |
| 24x2 | confidential signer | full+2tree | yes | 3453 | 43 | 3 | - | 1777 | 1769 | yes |  |
| 24x2 | confidential in-proof | min | yes | 2849 | 32 | 1 | - | 1675 | 1713 | yes |  |
| 24x2 | confidential in-proof | full | yes | 3412 | 41 | 2 | 7 | 1866 | 1862 | yes |  |
| 24x2 | confidential in-proof | full+spl | yes | 3780 | 48 | 2 | - | 1998 | 1866 | yes |  |
| 24x2 | confidential in-proof | full+data | yes | 3486 | 41 | 2 | - | 1940 | 1932 | yes |  |
| 24x2 | confidential in-proof | full+reply | yes | 3412 | 41 | 2 | - | 1866 | 1862 | yes |  |
| 24x2 | confidential in-proof | full+2tree | yes | 3452 | 42 | 2 | - | 1873 | 1866 | yes |  |
| 24x2 | anonymous hidden | min | yes | 2881 | 32 | 1 | - | 1707 | 1785 | yes |  |
| 24x2 | anonymous hidden | full | yes | 3348 | 38 | 2 | 7 | 1898 | 1934 | yes |  |
| 24x2 | anonymous hidden | full+spl | yes | 3716 | 45 | 2 | - | 2030 | 1938 | yes |  |
| 24x2 | anonymous hidden | full+data | yes | 3422 | 38 | 2 | - | 1972 | 2004 | yes |  |
| 24x2 | anonymous hidden | full+reply | yes | 3447 | 38 | 2 | - | 1997 | 2031 | yes |  |
| 24x2 | anonymous hidden | full+2tree | yes | 3388 | 39 | 2 | - | 1905 | 1938 | yes |  |
| 24x2 | anonymous public owners | min | yes | 2818 | 32 | 1 | - | 1643 | 1688 | yes |  |
| 24x2 | anonymous public owners | full | yes | 3477 | 42 | 3 | 7 | 1834 | 1837 | yes |  |
| 24x2 | anonymous public owners | full+spl | yes | 3845 | 49 | 3 | - | 1966 | 1841 | yes |  |
| 24x2 | anonymous public owners | full+data | yes | 3551 | 42 | 3 | - | 1908 | 1907 | yes |  |
| 24x2 | anonymous public owners | full+reply | yes | 3576 | 42 | 3 | - | 1933 | 1934 | yes |  |
| 24x2 | anonymous public owners | full+2tree | yes | 3517 | 43 | 3 | - | 1841 | 1841 | yes |  |
| 24x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 24x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 24x4 | confidential signer | min | yes | 3108 | 32 | 1 | - | 1933 | 2042 | yes |  |
| 24x4 | confidential signer | full | yes | 3767 | 42 | 3 | 4 | 2124 | 2191 | yes |  |
| 24x4 | confidential signer | full+spl | yes | 4135 | 49 | 3 | - | 2256 | 2195 | no | 39 B |
| 24x4 | confidential signer | full+data | yes | 3915 | 42 | 3 | - | 2272 | 2331 | yes |  |
| 24x4 | confidential signer | full+reply | yes | 3767 | 42 | 3 | - | 2124 | 2191 | yes |  |
| 24x4 | confidential signer | full+2tree | yes | 3807 | 43 | 3 | - | 2131 | 2195 | yes |  |
| 24x4 | confidential in-proof | min | yes | 3203 | 32 | 1 | - | 2029 | 2139 | yes |  |
| 24x4 | confidential in-proof | full | yes | 3766 | 41 | 2 | 3 | 2220 | 2288 | yes |  |
| 24x4 | confidential in-proof | full+spl | yes | 4134 | 48 | 2 | - | 2352 | 2292 | no | 38 B |
| 24x4 | confidential in-proof | full+data | yes | 3914 | 41 | 2 | - | 2368 | 2428 | yes |  |
| 24x4 | confidential in-proof | full+reply | yes | 3766 | 41 | 2 | - | 2220 | 2288 | yes |  |
| 24x4 | confidential in-proof | full+2tree | yes | 3806 | 42 | 2 | - | 2227 | 2292 | yes |  |
| 24x4 | anonymous hidden | min | yes | 3235 | 32 | 1 | - | 2061 | 2211 | yes |  |
| 24x4 | anonymous hidden | full | yes | 3702 | 38 | 2 | 4 | 2252 | 2360 | yes |  |
| 24x4 | anonymous hidden | full+spl | yes | 4070 | 45 | 2 | - | 2384 | 2364 | yes |  |
| 24x4 | anonymous hidden | full+data | yes | 3850 | 38 | 2 | - | 2400 | 2500 | yes |  |
| 24x4 | anonymous hidden | full+reply | yes | 3995 | 38 | 2 | - | 2545 | 2651 | yes |  |
| 24x4 | anonymous hidden | full+2tree | yes | 3742 | 39 | 2 | - | 2259 | 2364 | yes |  |
| 24x4 | anonymous public owners | min | yes | 3172 | 32 | 1 | - | 1997 | 2114 | yes |  |
| 24x4 | anonymous public owners | full | yes | 3831 | 42 | 3 | 3 | 2188 | 2263 | yes |  |
| 24x4 | anonymous public owners | full+spl | yes | 4199 | 49 | 3 | - | 2320 | 2267 | no | 103 B |
| 24x4 | anonymous public owners | full+data | yes | 3979 | 42 | 3 | - | 2336 | 2403 | yes |  |
| 24x4 | anonymous public owners | full+reply | yes | 4124 | 42 | 3 | - | 2481 | 2554 | no | 28 B |
| 24x4 | anonymous public owners | full+2tree | yes | 3871 | 43 | 3 | - | 2195 | 2267 | yes |  |
| 24x4 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 24x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 32x2 | confidential signer | min | yes | 3274 | 40 | 1 | - | 1835 | 1880 | yes |  |
| 32x2 | confidential signer | full | yes | 3933 | 50 | 3 | 2 | 2026 | 2029 | yes |  |
| 32x2 | confidential signer | full+spl | yes | 4301 | 57 | 3 | - | 2158 | 2033 | no | 205 B |
| 32x2 | confidential signer | full+data | yes | 4007 | 50 | 3 | - | 2100 | 2099 | yes |  |
| 32x2 | confidential signer | full+reply | yes | 3933 | 50 | 3 | - | 2026 | 2029 | yes |  |
| 32x2 | confidential signer | full+2tree | yes | 3974 | 51 | 3 | - | 2034 | 2033 | yes |  |
| 32x2 | confidential in-proof | min | yes | 3369 | 40 | 1 | - | 1931 | 1977 | yes |  |
| 32x2 | confidential in-proof | full | yes | 3932 | 49 | 2 | 1 | 2122 | 2126 | yes |  |
| 32x2 | confidential in-proof | full+spl | yes | 4300 | 56 | 2 | - | 2254 | 2130 | no | 204 B |
| 32x2 | confidential in-proof | full+data | yes | 4006 | 49 | 2 | - | 2196 | 2196 | yes |  |
| 32x2 | confidential in-proof | full+reply | yes | 3932 | 49 | 2 | - | 2122 | 2126 | yes |  |
| 32x2 | confidential in-proof | full+2tree | yes | 3973 | 50 | 2 | - | 2130 | 2130 | yes |  |
| 32x2 | anonymous hidden | min | yes | 3401 | 40 | 1 | - | 1963 | 2049 | yes |  |
| 32x2 | anonymous hidden | full | yes | 3868 | 46 | 2 | 2 | 2154 | 2198 | yes |  |
| 32x2 | anonymous hidden | full+spl | yes | 4236 | 53 | 2 | - | 2286 | 2202 | no | 140 B |
| 32x2 | anonymous hidden | full+data | yes | 3942 | 46 | 2 | - | 2228 | 2268 | yes |  |
| 32x2 | anonymous hidden | full+reply | yes | 3967 | 46 | 2 | - | 2253 | 2295 | yes |  |
| 32x2 | anonymous hidden | full+2tree | yes | 3909 | 47 | 2 | - | 2162 | 2202 | yes |  |
| 32x2 | anonymous public owners | min | yes | 3338 | 40 | 1 | - | 1899 | 1952 | yes |  |
| 32x2 | anonymous public owners | full | yes | 3997 | 50 | 3 | 2 | 2090 | 2101 | yes |  |
| 32x2 | anonymous public owners | full+spl | yes | 4365 | 57 | 3 | - | 2222 | 2105 | no | 269 B |
| 32x2 | anonymous public owners | full+data | yes | 4071 | 50 | 3 | - | 2164 | 2171 | yes |  |
| 32x2 | anonymous public owners | full+reply | yes | 4096 | 50 | 3 | - | 2189 | 2198 | yes |  |
| 32x2 | anonymous public owners | full+2tree | yes | 4038 | 51 | 3 | - | 2098 | 2105 | yes |  |
| 32x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 32x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 40x2 | confidential signer | min | yes | 3794 | 48 | 1 | - | 2091 | 2144 | yes |  |
| 40x2 | confidential signer | full | yes | 4453 | 58 | 3 | none | 2282 | 2293 | no | 357 B |
| 40x2 | confidential signer | full+spl | yes | 4821 | 65 | 3 | - | 2414 | 2297 | no | 725 B, 1 accounts |
| 40x2 | confidential signer | full+data | yes | 4527 | 58 | 3 | - | 2356 | 2363 | no | 431 B |
| 40x2 | confidential signer | full+reply | yes | 4453 | 58 | 3 | - | 2282 | 2293 | no | 357 B |
| 40x2 | confidential signer | full+2tree | yes | 4495 | 59 | 3 | - | 2291 | 2297 | no | 399 B |
| 40x2 | confidential in-proof | min | yes | 3889 | 48 | 1 | - | 2187 | 2241 | yes |  |
| 40x2 | confidential in-proof | full | yes | 4452 | 57 | 2 | none | 2378 | 2390 | no | 356 B |
| 40x2 | confidential in-proof | full+spl | yes | 4820 | 64 | 2 | - | 2510 | 2394 | no | 724 B |
| 40x2 | confidential in-proof | full+data | yes | 4526 | 57 | 2 | - | 2452 | 2460 | no | 430 B |
| 40x2 | confidential in-proof | full+reply | yes | 4452 | 57 | 2 | - | 2378 | 2390 | no | 356 B |
| 40x2 | confidential in-proof | full+2tree | yes | 4494 | 58 | 2 | - | 2387 | 2394 | no | 398 B |
| 40x2 | anonymous hidden | min | yes | 3921 | 48 | 1 | - | 2219 | 2313 | yes |  |
| 40x2 | anonymous hidden | full | yes | 4388 | 54 | 2 | none | 2410 | 2462 | no | 292 B |
| 40x2 | anonymous hidden | full+spl | yes | 4756 | 61 | 2 | - | 2542 | 2466 | no | 660 B |
| 40x2 | anonymous hidden | full+data | yes | 4462 | 54 | 2 | - | 2484 | 2532 | no | 366 B |
| 40x2 | anonymous hidden | full+reply | yes | 4487 | 54 | 2 | - | 2509 | 2559 | no | 391 B |
| 40x2 | anonymous hidden | full+2tree | yes | 4430 | 55 | 2 | - | 2419 | 2466 | no | 334 B |
| 40x2 | anonymous public owners | min | yes | 3858 | 48 | 1 | - | 2155 | 2216 | yes |  |
| 40x2 | anonymous public owners | full | yes | 4517 | 58 | 3 | none | 2346 | 2365 | no | 421 B |
| 40x2 | anonymous public owners | full+spl | yes | 4885 | 65 | 3 | - | 2478 | 2369 | no | 789 B, 1 accounts |
| 40x2 | anonymous public owners | full+data | yes | 4591 | 58 | 3 | - | 2420 | 2435 | no | 495 B |
| 40x2 | anonymous public owners | full+reply | yes | 4616 | 58 | 3 | - | 2445 | 2462 | no | 520 B |
| 40x2 | anonymous public owners | full+2tree | yes | 4559 | 59 | 3 | - | 2355 | 2369 | no | 463 B |
| 40x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 40x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 48x2 | confidential signer | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 48x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 49x2 | confidential signer | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 49x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x1 | merge | ring | yes | 1729 | 16 | 1 | - | - | - | yes |  |
| 8x1 | merge | owner signed | yes | 1826 | 17 | 2 | - | - | - | yes |  |
| 24x1 | merge | ring | yes | 2769 | 32 | 1 | - | - | - | yes |  |
| 24x1 | merge | owner signed | yes | 2866 | 33 | 2 | - | - | - | yes |  |
| 52x1 | merge | ring | yes | 4589 | 60 | 1 | - | - | - | no | 493 B |
| 52x1 | merge | owner signed | yes | 4686 | 61 | 2 | - | - | - | no | 590 B |

Entry capacities at two outputs. Each row is the widest owner run one transaction clears at that leg count, rows = run + a hidden sender. A confidential leg names its own destination, a hidden sender's legs name one. over: what stops the next owner.
| shape | rail | load | legs | leg kind | owner run | rows | accounts | signatures | frame | SPP CPI | fits | over |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 12x2 | confidential entry | min | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+data+reply | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | min | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+data+reply | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | min | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+data+reply | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | min | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+data+reply | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | min | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+data+reply | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | min | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+data+reply | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | min | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+data+reply | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | min | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+data+reply | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | min | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+data+reply | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | min | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+data+reply | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | min | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+data+reply | 0 | sol | none | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | min | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full | 0 | spl | none | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+data+reply | 0 | spl | none | - | - | - | - | - | no | catalog key |

Maxima. Load max is full with recipient payloads, every extension but the cache, two SPP input trees, replies on anonymous frames. Each row widens one axis from that load to the largest value one transaction clears. reply: the reply length. messages: empty application messages. message bytes: one application message. over: what refuses one more step. Heap is not measured here.
| shape | rail | axis | widest | frame | SPP CPI | bytes | accounts | signatures | over |
|---|---|---|---|---|---|---|---|---|---|
| 1x2 | confidential signer | messages | 57 | 3149 | 3082 | 4066 | 20 | 3 | 4 B |
| 1x2 | confidential signer | message bytes | 1934 | 3179 | 3112 | 4096 | 20 | 3 | 1 B |
| 1x2 | confidential in-proof | messages | 57 | 3245 | 3179 | 4065 | 19 | 2 | 3 B |
| 1x2 | confidential in-proof | message bytes | 1935 | 3276 | 3210 | 4096 | 19 | 2 | 1 B |
| 1x2 | anonymous hidden | reply | 2032 | 3372 | 3345 | 4096 | 16 | 2 | 1 B |
| 1x2 | anonymous hidden | messages | 56 | 3342 | 3314 | 4066 | 16 | 2 | 4 B |
| 1x2 | anonymous hidden | message bytes | 1900 | 3372 | 3344 | 4096 | 16 | 2 | 1 B |
| 1x2 | anonymous public owners | reply | 1903 | 3179 | 3119 | 4096 | 20 | 3 | 1 B |
| 1x2 | anonymous public owners | messages | 53 | 3176 | 3115 | 4093 | 20 | 3 | 31 B |
| 1x2 | anonymous public owners | message bytes | 1771 | 3179 | 3118 | 4096 | 20 | 3 | 1 B |
| 1x2 | confidential entry | messages | 53 | 3157 | 3090 | 4074 | 20 | 3 | 12 B |
| 1x2 | confidential entry | message bytes | 1790 | 3179 | 3112 | 4096 | 20 | 3 | 1 B |
| 1x2 | anonymous entry | reply | 1856 | 3372 | 3313 | 4096 | 16 | 2 | 1 B |
| 1x2 | anonymous entry | messages | 51 | 3348 | 3288 | 4072 | 16 | 2 | 10 B |
| 1x2 | anonymous entry | message bytes | 1724 | 3372 | 3312 | 4096 | 16 | 2 | 1 B |
| 1x4 | confidential signer | messages | 45 | 3169 | 3170 | 4086 | 20 | 3 | 24 B |
| 1x4 | confidential signer | message bytes | 1506 | 3179 | 3180 | 4096 | 20 | 3 | 1 B |
| 1x4 | confidential in-proof | messages | 45 | 3265 | 3267 | 4085 | 19 | 2 | 23 B |
| 1x4 | confidential in-proof | message bytes | 1507 | 3276 | 3278 | 4096 | 19 | 2 | 1 B |
| 1x4 | anonymous hidden | reply | 534 | 3370 | 3411 | 4094 | 16 | 2 | 1 B |
| 1x4 | anonymous hidden | messages | 38 | 3352 | 3392 | 4076 | 16 | 2 | 14 B |
| 1x4 | anonymous hidden | message bytes | 1278 | 3372 | 3412 | 4096 | 16 | 2 | 1 B |
| 1x4 | anonymous public owners | reply | 491 | 3177 | 3185 | 4094 | 20 | 3 | 1 B |
| 1x4 | anonymous public owners | messages | 34 | 3152 | 3159 | 4069 | 20 | 3 | 7 B |
| 1x4 | anonymous public owners | message bytes | 1149 | 3179 | 3186 | 4096 | 20 | 3 | 1 B |
| 1x4 | confidential entry | messages | 41 | 3177 | 3178 | 4094 | 20 | 3 | 32 B |
| 1x4 | confidential entry | message bytes | 1362 | 3179 | 3180 | 4096 | 20 | 3 | 1 B |
| 1x4 | anonymous entry | reply | 476 | 3372 | 3381 | 4096 | 16 | 2 | 3 B |
| 1x4 | anonymous entry | messages | 33 | 3358 | 3366 | 4082 | 16 | 2 | 20 B |
| 1x4 | anonymous entry | message bytes | 1102 | 3372 | 3380 | 4096 | 16 | 2 | 1 B |
| 1x8 | confidential signer | messages | 20 | 3175 | 3312 | 4092 | 20 | 3 | 30 B |
| 1x8 | confidential signer | message bytes | 650 | 3179 | 3316 | 4096 | 20 | 3 | 1 B |
| 1x8 | confidential in-proof | messages | 20 | 3271 | 3409 | 4091 | 19 | 2 | 29 B |
| 1x8 | confidential in-proof | message bytes | 651 | 3276 | 3414 | 4096 | 19 | 2 | 1 B |
| 1x8 | anonymous hidden | reply | 106 | 3366 | 3543 | 4090 | 16 | 2 | 1 B |
| 1x8 | anonymous hidden | messages | 2 | 3372 | 3548 | 4096 | 16 | 2 | 34 B |
| 1x8 | anonymous hidden | message bytes | 34 | 3372 | 3548 | 4096 | 16 | 2 | 1 B |
| 1x8 | anonymous public owners | reply | none | - | - | - | - | - | 60 B |
| 1x8 | anonymous public owners | messages | none | - | - | - | - | - | 60 B |
| 1x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 95 B |
| 1x8 | confidential entry | messages | 15 | 3149 | 3286 | 4066 | 20 | 3 | 4 B |
| 1x8 | confidential entry | message bytes | 506 | 3179 | 3316 | 4096 | 20 | 3 | 1 B |
| 1x8 | anonymous entry | reply | none | - | - | - | - | - | 107 B |
| 1x8 | anonymous entry | messages | none | - | - | - | - | - | 107 B |
| 1x8 | anonymous entry | message bytes | none | - | - | - | - | - | 142 B |
| 2x2 | confidential signer | messages | 55 | 3113 | 3047 | 4063 | 21 | 3 | 1 B |
| 2x2 | confidential signer | message bytes | 1869 | 3146 | 3080 | 4096 | 21 | 3 | 1 B |
| 2x2 | confidential in-proof | messages | 56 | 3243 | 3178 | 4096 | 20 | 2 | 34 B |
| 2x2 | confidential in-proof | message bytes | 1870 | 3243 | 3178 | 4096 | 20 | 2 | 1 B |
| 2x2 | anonymous hidden | reply | 1967 | 3339 | 3313 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous hidden | messages | 54 | 3306 | 3279 | 4063 | 17 | 2 | 1 B |
| 2x2 | anonymous hidden | message bytes | 1835 | 3339 | 3312 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous public owners | reply | 1838 | 3146 | 3087 | 4096 | 21 | 3 | 1 B |
| 2x2 | anonymous public owners | messages | 51 | 3140 | 3080 | 4090 | 21 | 3 | 28 B |
| 2x2 | anonymous public owners | message bytes | 1706 | 3146 | 3086 | 4096 | 21 | 3 | 1 B |
| 2x2 | confidential entry | messages | 47 | 3121 | 3055 | 4071 | 21 | 3 | 9 B |
| 2x2 | confidential entry | message bytes | 1589 | 3146 | 3080 | 4096 | 21 | 3 | 1 B |
| 2x2 | anonymous entry | reply | 1655 | 3339 | 3281 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous entry | messages | 45 | 3312 | 3253 | 4069 | 17 | 2 | 7 B |
| 2x2 | anonymous entry | message bytes | 1523 | 3339 | 3280 | 4096 | 17 | 2 | 1 B |
| 2x2 | delegate | messages | 63 | 3337 | 3319 | 4091 | 17 | 2 | 29 B |
| 2x2 | delegate | message bytes | 2113 | 3342 | 3324 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous delegate | reply | 2082 | 3342 | 3331 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous delegate | messages | 58 | 3330 | 3318 | 4084 | 17 | 2 | 22 B |
| 2x2 | anonymous delegate | message bytes | 1950 | 3342 | 3330 | 4096 | 17 | 2 | 1 B |
| 2x4 | confidential signer | messages | 43 | 3133 | 3135 | 4083 | 21 | 3 | 21 B |
| 2x4 | confidential signer | message bytes | 1441 | 3146 | 3148 | 4096 | 21 | 3 | 1 B |
| 2x4 | confidential in-proof | messages | 43 | 3229 | 3232 | 4082 | 20 | 2 | 20 B |
| 2x4 | confidential in-proof | message bytes | 1442 | 3243 | 3246 | 4096 | 20 | 2 | 1 B |
| 2x4 | anonymous hidden | reply | 513 | 3339 | 3381 | 4096 | 17 | 2 | 3 B |
| 2x4 | anonymous hidden | messages | 36 | 3316 | 3357 | 4073 | 17 | 2 | 11 B |
| 2x4 | anonymous hidden | message bytes | 1213 | 3339 | 3380 | 4096 | 17 | 2 | 1 B |
| 2x4 | anonymous public owners | reply | 470 | 3146 | 3155 | 4096 | 21 | 3 | 3 B |
| 2x4 | anonymous public owners | messages | 32 | 3116 | 3124 | 4066 | 21 | 3 | 4 B |
| 2x4 | anonymous public owners | message bytes | 1084 | 3146 | 3154 | 4096 | 21 | 3 | 1 B |
| 2x4 | confidential entry | messages | 35 | 3141 | 3143 | 4091 | 21 | 3 | 29 B |
| 2x4 | confidential entry | message bytes | 1161 | 3146 | 3148 | 4096 | 21 | 3 | 1 B |
| 2x4 | anonymous entry | reply | 409 | 3339 | 3349 | 4096 | 17 | 2 | 3 B |
| 2x4 | anonymous entry | messages | 27 | 3322 | 3331 | 4079 | 17 | 2 | 17 B |
| 2x4 | anonymous entry | message bytes | 901 | 3339 | 3348 | 4096 | 17 | 2 | 1 B |
| 1x16 | confidential signer | messages | none | - | - | - | - | - | 1028 B |
| 1x16 | confidential signer | message bytes | none | - | - | - | - | - | 1063 B |
| 1x16 | confidential in-proof | messages | none | - | - | - | - | - | 1027 B |
| 1x16 | confidential in-proof | message bytes | none | - | - | - | - | - | 1062 B |
| 1x16 | anonymous hidden | reply | none | - | - | - | - | - | 2420 B |
| 1x16 | anonymous hidden | messages | none | - | - | - | - | - | 2420 B |
| 1x16 | anonymous hidden | message bytes | none | - | - | - | - | - | 2455 B |
| 1x16 | anonymous public owners | reply | none | - | - | - | - | - | 2549 B |
| 1x16 | anonymous public owners | messages | none | - | - | - | - | - | 2549 B |
| 1x16 | anonymous public owners | message bytes | none | - | - | - | - | - | 2584 B |
| 2x8 | confidential signer | messages | 18 | 3139 | 3277 | 4089 | 21 | 3 | 27 B |
| 2x8 | confidential signer | message bytes | 585 | 3146 | 3284 | 4096 | 21 | 3 | 1 B |
| 2x8 | confidential in-proof | messages | 18 | 3235 | 3374 | 4088 | 20 | 2 | 26 B |
| 2x8 | confidential in-proof | message bytes | 586 | 3243 | 3382 | 4096 | 20 | 2 | 1 B |
| 2x8 | anonymous hidden | reply | 97 | 3335 | 3513 | 4092 | 17 | 2 | 3 B |
| 2x8 | anonymous hidden | messages | 0 | 3335 | 3513 | 4092 | 17 | 2 | 31 B |
| 2x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 31 B |
| 2x8 | anonymous public owners | reply | none | - | - | - | - | - | 125 B |
| 2x8 | anonymous public owners | messages | none | - | - | - | - | - | 125 B |
| 2x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 160 B |
| 2x8 | confidential entry | messages | 9 | 3113 | 3251 | 4063 | 21 | 3 | 1 B |
| 2x8 | confidential entry | message bytes | 305 | 3146 | 3284 | 4096 | 21 | 3 | 1 B |
| 2x8 | anonymous entry | reply | none | - | - | - | - | - | 308 B |
| 2x8 | anonymous entry | messages | none | - | - | - | - | - | 308 B |
| 2x8 | anonymous entry | message bytes | none | - | - | - | - | - | 343 B |
| 3x2 | confidential signer | messages | 54 | 3111 | 3046 | 4094 | 22 | 3 | 32 B |
| 3x2 | confidential signer | message bytes | 1804 | 3113 | 3048 | 4096 | 22 | 3 | 1 B |
| 3x2 | confidential in-proof | messages | 54 | 3207 | 3143 | 4093 | 21 | 2 | 31 B |
| 3x2 | confidential in-proof | message bytes | 1805 | 3210 | 3146 | 4096 | 21 | 2 | 1 B |
| 3x2 | anonymous hidden | reply | 1902 | 3306 | 3281 | 4096 | 18 | 2 | 1 B |
| 3x2 | anonymous hidden | messages | 53 | 3304 | 3278 | 4094 | 18 | 2 | 32 B |
| 3x2 | anonymous hidden | message bytes | 1770 | 3306 | 3280 | 4096 | 18 | 2 | 1 B |
| 3x2 | anonymous public owners | reply | 1773 | 3113 | 3055 | 4096 | 22 | 3 | 1 B |
| 3x2 | anonymous public owners | messages | 49 | 3104 | 3045 | 4087 | 22 | 3 | 25 B |
| 3x2 | anonymous public owners | message bytes | 1641 | 3113 | 3054 | 4096 | 22 | 3 | 1 B |
| 3x2 | confidential entry | messages | 41 | 3085 | 3020 | 4068 | 22 | 3 | 6 B |
| 3x2 | confidential entry | message bytes | 1388 | 3113 | 3048 | 4096 | 22 | 3 | 1 B |
| 3x2 | anonymous entry | reply | 1454 | 3306 | 3249 | 4096 | 18 | 2 | 1 B |
| 3x2 | anonymous entry | messages | 39 | 3276 | 3218 | 4066 | 18 | 2 | 4 B |
| 3x2 | anonymous entry | message bytes | 1322 | 3306 | 3248 | 4096 | 18 | 2 | 1 B |
| 3x4 | confidential signer | messages | 41 | 3097 | 3100 | 4080 | 22 | 3 | 18 B |
| 3x4 | confidential signer | message bytes | 1376 | 3113 | 3116 | 4096 | 22 | 3 | 1 B |
| 3x4 | confidential in-proof | messages | 41 | 3193 | 3197 | 4079 | 21 | 2 | 17 B |
| 3x4 | confidential in-proof | message bytes | 1377 | 3210 | 3214 | 4096 | 21 | 2 | 1 B |
| 3x4 | anonymous hidden | reply | 491 | 3305 | 3348 | 4095 | 18 | 2 | 2 B |
| 3x4 | anonymous hidden | messages | 34 | 3280 | 3322 | 4070 | 18 | 2 | 8 B |
| 3x4 | anonymous hidden | message bytes | 1148 | 3306 | 3348 | 4096 | 18 | 2 | 1 B |
| 3x4 | anonymous public owners | reply | 448 | 3112 | 3122 | 4095 | 22 | 3 | 2 B |
| 3x4 | anonymous public owners | messages | 30 | 3080 | 3089 | 4063 | 22 | 3 | 1 B |
| 3x4 | anonymous public owners | message bytes | 1019 | 3113 | 3122 | 4096 | 22 | 3 | 1 B |
| 3x4 | confidential entry | messages | 29 | 3105 | 3108 | 4088 | 22 | 3 | 26 B |
| 3x4 | confidential entry | message bytes | 960 | 3113 | 3116 | 4096 | 22 | 3 | 1 B |
| 3x4 | anonymous entry | reply | 342 | 3306 | 3317 | 4096 | 18 | 2 | 3 B |
| 3x4 | anonymous entry | messages | 21 | 3286 | 3296 | 4076 | 18 | 2 | 14 B |
| 3x4 | anonymous entry | message bytes | 700 | 3306 | 3316 | 4096 | 18 | 2 | 1 B |
| 2x16 | confidential signer | messages | none | - | - | - | - | - | 1093 B |
| 2x16 | confidential signer | message bytes | none | - | - | - | - | - | 1128 B |
| 2x16 | confidential in-proof | messages | none | - | - | - | - | - | 1092 B |
| 2x16 | confidential in-proof | message bytes | none | - | - | - | - | - | 1127 B |
| 2x16 | anonymous hidden | reply | none | - | - | - | - | - | 2485 B |
| 2x16 | anonymous hidden | messages | none | - | - | - | - | - | 2485 B |
| 2x16 | anonymous hidden | message bytes | none | - | - | - | - | - | 2520 B |
| 2x16 | anonymous public owners | reply | none | - | - | - | - | - | 2614 B |
| 2x16 | anonymous public owners | messages | none | - | - | - | - | - | 2614 B |
| 2x16 | anonymous public owners | message bytes | none | - | - | - | - | - | 2649 B |
| 3x8 | confidential signer | messages | 16 | 3103 | 3242 | 4086 | 22 | 3 | 24 B |
| 3x8 | confidential signer | message bytes | 520 | 3113 | 3252 | 4096 | 22 | 3 | 1 B |
| 3x8 | confidential in-proof | messages | 16 | 3199 | 3339 | 4085 | 21 | 2 | 23 B |
| 3x8 | confidential in-proof | message bytes | 521 | 3210 | 3350 | 4096 | 21 | 2 | 1 B |
| 3x8 | anonymous hidden | reply | none | - | - | - | - | - | 61 B |
| 3x8 | anonymous hidden | messages | none | - | - | - | - | - | 61 B |
| 3x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 96 B |
| 3x8 | anonymous public owners | reply | none | - | - | - | - | - | 190 B |
| 3x8 | anonymous public owners | messages | none | - | - | - | - | - | 190 B |
| 3x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 225 B |
| 3x8 | confidential entry | messages | 4 | 3111 | 3250 | 4094 | 22 | 3 | 32 B |
| 3x8 | confidential entry | message bytes | 104 | 3113 | 3252 | 4096 | 22 | 3 | 1 B |
| 3x8 | anonymous entry | reply | none | - | - | - | - | - | 509 B |
| 3x8 | anonymous entry | messages | none | - | - | - | - | - | 509 B |
| 3x8 | anonymous entry | message bytes | none | - | - | - | - | - | 544 B |
| 4x2 | confidential signer | messages | 52 | 3075 | 3011 | 4091 | 23 | 3 | 29 B |
| 4x2 | confidential signer | message bytes | 1739 | 3080 | 3016 | 4096 | 23 | 3 | 1 B |
| 4x2 | confidential in-proof | messages | 52 | 3171 | 3108 | 4090 | 22 | 2 | 28 B |
| 4x2 | confidential in-proof | message bytes | 1740 | 3177 | 3114 | 4096 | 22 | 2 | 1 B |
| 4x2 | anonymous hidden | reply | 1837 | 3273 | 3249 | 4096 | 19 | 2 | 1 B |
| 4x2 | anonymous hidden | messages | 51 | 3268 | 3243 | 4091 | 19 | 2 | 29 B |
| 4x2 | anonymous hidden | message bytes | 1705 | 3273 | 3248 | 4096 | 19 | 2 | 1 B |
| 4x2 | anonymous public owners | reply | 1708 | 3080 | 3023 | 4096 | 23 | 3 | 1 B |
| 4x2 | anonymous public owners | messages | 47 | 3068 | 3010 | 4084 | 23 | 3 | 22 B |
| 4x2 | anonymous public owners | message bytes | 1576 | 3080 | 3022 | 4096 | 23 | 3 | 1 B |
| 4x2 | confidential entry | messages | 35 | 3049 | 2985 | 4065 | 23 | 3 | 3 B |
| 4x2 | confidential entry | message bytes | 1187 | 3080 | 3016 | 4096 | 23 | 3 | 1 B |
| 4x2 | anonymous entry | reply | 1253 | 3273 | 3217 | 4096 | 19 | 2 | 1 B |
| 4x2 | anonymous entry | messages | 33 | 3240 | 3183 | 4063 | 19 | 2 | 1 B |
| 4x2 | anonymous entry | message bytes | 1121 | 3273 | 3216 | 4096 | 19 | 2 | 1 B |
| 4x4 | confidential signer | messages | 39 | 3061 | 3065 | 4077 | 23 | 3 | 15 B |
| 4x4 | confidential signer | message bytes | 1311 | 3080 | 3084 | 4096 | 23 | 3 | 1 B |
| 4x4 | confidential in-proof | messages | 39 | 3157 | 3162 | 4076 | 22 | 2 | 14 B |
| 4x4 | confidential in-proof | message bytes | 1312 | 3177 | 3182 | 4096 | 22 | 2 | 1 B |
| 4x4 | anonymous hidden | reply | 469 | 3271 | 3315 | 4094 | 19 | 2 | 1 B |
| 4x4 | anonymous hidden | messages | 32 | 3244 | 3287 | 4067 | 19 | 2 | 5 B |
| 4x4 | anonymous hidden | message bytes | 1083 | 3273 | 3316 | 4096 | 19 | 2 | 1 B |
| 4x4 | anonymous public owners | reply | 426 | 3078 | 3089 | 4094 | 23 | 3 | 1 B |
| 4x4 | anonymous public owners | messages | 29 | 3078 | 3088 | 4094 | 23 | 3 | 32 B |
| 4x4 | anonymous public owners | message bytes | 954 | 3080 | 3090 | 4096 | 23 | 3 | 1 B |
| 4x4 | confidential entry | messages | 23 | 3069 | 3073 | 4085 | 23 | 3 | 23 B |
| 4x4 | confidential entry | message bytes | 759 | 3080 | 3084 | 4096 | 23 | 3 | 1 B |
| 4x4 | anonymous entry | reply | 275 | 3273 | 3285 | 4096 | 19 | 2 | 3 B |
| 4x4 | anonymous entry | messages | 15 | 3250 | 3261 | 4073 | 19 | 2 | 11 B |
| 4x4 | anonymous entry | message bytes | 499 | 3273 | 3284 | 4096 | 19 | 2 | 1 B |
| 4x4 | delegate | messages | 46 | 3251 | 3303 | 4071 | 19 | 2 | 9 B |
| 4x4 | delegate | message bytes | 1555 | 3276 | 3328 | 4096 | 19 | 2 | 1 B |
| 4x4 | anonymous delegate | reply | 508 | 3276 | 3335 | 4096 | 19 | 2 | 3 B |
| 4x4 | anonymous delegate | messages | 36 | 3268 | 3326 | 4088 | 19 | 2 | 26 B |
| 4x4 | anonymous delegate | message bytes | 1198 | 3276 | 3334 | 4096 | 19 | 2 | 1 B |
| 4x8 | confidential signer | messages | 14 | 3067 | 3207 | 4083 | 23 | 3 | 21 B |
| 4x8 | confidential signer | message bytes | 455 | 3080 | 3220 | 4096 | 23 | 3 | 1 B |
| 4x8 | confidential in-proof | messages | 14 | 3163 | 3304 | 4082 | 22 | 2 | 20 B |
| 4x8 | confidential in-proof | message bytes | 456 | 3177 | 3318 | 4096 | 22 | 2 | 1 B |
| 4x8 | anonymous hidden | reply | none | - | - | - | - | - | 126 B |
| 4x8 | anonymous hidden | messages | none | - | - | - | - | - | 126 B |
| 4x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 161 B |
| 4x8 | anonymous public owners | reply | none | - | - | - | - | - | 255 B |
| 4x8 | anonymous public owners | messages | none | - | - | - | - | - | 255 B |
| 4x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 290 B |
| 4x8 | confidential entry | messages | none | - | - | - | - | - | 62 B |
| 4x8 | confidential entry | message bytes | none | - | - | - | - | - | 97 B |
| 4x8 | anonymous entry | reply | none | - | - | - | - | - | 710 B |
| 4x8 | anonymous entry | messages | none | - | - | - | - | - | 710 B |
| 4x8 | anonymous entry | message bytes | none | - | - | - | - | - | 745 B |
| 5x2 | confidential signer | messages | 50 | 3039 | 2976 | 4088 | 24 | 3 | 26 B |
| 5x2 | confidential signer | message bytes | 1674 | 3047 | 2984 | 4096 | 24 | 3 | 1 B |
| 5x2 | confidential in-proof | messages | 50 | 3135 | 3073 | 4087 | 23 | 2 | 25 B |
| 5x2 | confidential in-proof | message bytes | 1675 | 3144 | 3082 | 4096 | 23 | 2 | 1 B |
| 5x2 | anonymous hidden | reply | 1772 | 3240 | 3217 | 4096 | 20 | 2 | 1 B |
| 5x2 | anonymous hidden | messages | 49 | 3232 | 3208 | 4088 | 20 | 2 | 26 B |
| 5x2 | anonymous hidden | message bytes | 1640 | 3240 | 3216 | 4096 | 20 | 2 | 1 B |
| 5x2 | anonymous public owners | reply | 1643 | 3047 | 2991 | 4096 | 24 | 3 | 1 B |
| 5x2 | anonymous public owners | messages | 45 | 3032 | 2975 | 4081 | 24 | 3 | 19 B |
| 5x2 | anonymous public owners | message bytes | 1511 | 3047 | 2990 | 4096 | 24 | 3 | 1 B |
| 5x2 | confidential entry | messages | 30 | 3047 | 2984 | 4096 | 24 | 3 | 34 B |
| 5x2 | confidential entry | message bytes | 986 | 3047 | 2984 | 4096 | 24 | 3 | 1 B |
| 5x2 | anonymous entry | reply | 1052 | 3240 | 3185 | 4096 | 20 | 2 | 1 B |
| 5x2 | anonymous entry | messages | 28 | 3238 | 3182 | 4094 | 20 | 2 | 32 B |
| 5x2 | anonymous entry | message bytes | 920 | 3240 | 3184 | 4096 | 20 | 2 | 1 B |
| 5x4 | confidential signer | messages | 37 | 3025 | 3030 | 4074 | 24 | 3 | 12 B |
| 5x4 | confidential signer | message bytes | 1246 | 3047 | 3052 | 4096 | 24 | 3 | 1 B |
| 5x4 | confidential in-proof | messages | 37 | 3121 | 3127 | 4073 | 23 | 2 | 11 B |
| 5x4 | confidential in-proof | message bytes | 1247 | 3144 | 3150 | 4096 | 23 | 2 | 1 B |
| 5x4 | anonymous hidden | reply | 448 | 3240 | 3285 | 4096 | 20 | 2 | 3 B |
| 5x4 | anonymous hidden | messages | 30 | 3208 | 3252 | 4064 | 20 | 2 | 2 B |
| 5x4 | anonymous hidden | message bytes | 1018 | 3240 | 3284 | 4096 | 20 | 2 | 1 B |
| 5x4 | anonymous public owners | reply | 405 | 3047 | 3059 | 4096 | 24 | 3 | 3 B |
| 5x4 | anonymous public owners | messages | 27 | 3042 | 3053 | 4091 | 24 | 3 | 29 B |
| 5x4 | anonymous public owners | message bytes | 889 | 3047 | 3058 | 4096 | 24 | 3 | 1 B |
| 5x4 | confidential entry | messages | 17 | 3033 | 3038 | 4082 | 24 | 3 | 20 B |
| 5x4 | confidential entry | message bytes | 558 | 3047 | 3052 | 4096 | 24 | 3 | 1 B |
| 5x4 | anonymous entry | reply | 208 | 3240 | 3253 | 4096 | 20 | 2 | 3 B |
| 5x4 | anonymous entry | messages | 9 | 3214 | 3226 | 4070 | 20 | 2 | 8 B |
| 5x4 | anonymous entry | message bytes | 298 | 3240 | 3252 | 4096 | 20 | 2 | 1 B |
| 4x16 | confidential signer | messages | none | - | - | - | - | - | 1223 B |
| 4x16 | confidential signer | message bytes | none | - | - | - | - | - | 1258 B |
| 4x16 | confidential in-proof | messages | none | - | - | - | - | - | 1222 B |
| 4x16 | confidential in-proof | message bytes | none | - | - | - | - | - | 1257 B |
| 4x16 | anonymous hidden | reply | none | - | - | - | - | - | 2615 B |
| 4x16 | anonymous hidden | messages | none | - | - | - | - | - | 2615 B |
| 4x16 | anonymous hidden | message bytes | none | - | - | - | - | - | 2650 B |
| 4x16 | anonymous public owners | reply | none | - | - | - | - | - | 2744 B |
| 4x16 | anonymous public owners | messages | none | - | - | - | - | - | 2744 B |
| 4x16 | anonymous public owners | message bytes | none | - | - | - | - | - | 2779 B |
| 5x8 | confidential signer | messages | 12 | 3031 | 3172 | 4080 | 24 | 3 | 18 B |
| 5x8 | confidential signer | message bytes | 390 | 3047 | 3188 | 4096 | 24 | 3 | 1 B |
| 5x8 | confidential in-proof | messages | 12 | 3127 | 3269 | 4079 | 23 | 2 | 17 B |
| 5x8 | confidential in-proof | message bytes | 391 | 3144 | 3286 | 4096 | 23 | 2 | 1 B |
| 5x8 | anonymous hidden | reply | none | - | - | - | - | - | 191 B |
| 5x8 | anonymous hidden | messages | none | - | - | - | - | - | 191 B |
| 5x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 226 B |
| 5x8 | anonymous public owners | reply | none | - | - | - | - | - | 320 B |
| 5x8 | anonymous public owners | messages | none | - | - | - | - | - | 320 B |
| 5x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 355 B |
| 5x8 | confidential entry | messages | none | - | - | - | - | - | 263 B |
| 5x8 | confidential entry | message bytes | none | - | - | - | - | - | 298 B |
| 5x8 | anonymous entry | reply | none | - | - | - | - | - | 911 B |
| 5x8 | anonymous entry | messages | none | - | - | - | - | - | 911 B |
| 5x8 | anonymous entry | message bytes | none | - | - | - | - | - | 946 B |
| 6x2 | confidential signer | messages | 48 | 3003 | 2941 | 4085 | 25 | 3 | 23 B |
| 6x2 | confidential signer | message bytes | 1609 | 3014 | 2952 | 4096 | 25 | 3 | 1 B |
| 6x2 | confidential in-proof | messages | 48 | 3099 | 3038 | 4084 | 24 | 2 | 22 B |
| 6x2 | confidential in-proof | message bytes | 1610 | 3111 | 3050 | 4096 | 24 | 2 | 1 B |
| 6x2 | anonymous hidden | reply | 1707 | 3207 | 3185 | 4096 | 21 | 2 | 1 B |
| 6x2 | anonymous hidden | messages | 47 | 3196 | 3173 | 4085 | 21 | 2 | 23 B |
| 6x2 | anonymous hidden | message bytes | 1575 | 3207 | 3184 | 4096 | 21 | 2 | 1 B |
| 6x2 | anonymous public owners | reply | 1578 | 3014 | 2959 | 4096 | 25 | 3 | 1 B |
| 6x2 | anonymous public owners | messages | 43 | 2996 | 2940 | 4078 | 25 | 3 | 16 B |
| 6x2 | anonymous public owners | message bytes | 1446 | 3014 | 2958 | 4096 | 25 | 3 | 1 B |
| 6x2 | confidential entry | messages | 24 | 3011 | 2949 | 4093 | 25 | 3 | 31 B |
| 6x2 | confidential entry | message bytes | 785 | 3014 | 2952 | 4096 | 25 | 3 | 1 B |
| 6x2 | anonymous entry | reply | 851 | 3207 | 3153 | 4096 | 21 | 2 | 1 B |
| 6x2 | anonymous entry | messages | 22 | 3202 | 3147 | 4091 | 21 | 2 | 29 B |
| 6x2 | anonymous entry | message bytes | 719 | 3207 | 3152 | 4096 | 21 | 2 | 1 B |
| 6x4 | confidential signer | messages | 35 | 2989 | 2995 | 4071 | 25 | 3 | 9 B |
| 6x4 | confidential signer | message bytes | 1181 | 3014 | 3020 | 4096 | 25 | 3 | 1 B |
| 6x4 | confidential in-proof | messages | 35 | 3085 | 3092 | 4070 | 24 | 2 | 8 B |
| 6x4 | confidential in-proof | message bytes | 1182 | 3111 | 3118 | 4096 | 24 | 2 | 1 B |
| 6x4 | anonymous hidden | reply | 426 | 3206 | 3252 | 4095 | 21 | 2 | 2 B |
| 6x4 | anonymous hidden | messages | 29 | 3206 | 3251 | 4095 | 21 | 2 | 33 B |
| 6x4 | anonymous hidden | message bytes | 953 | 3207 | 3252 | 4096 | 21 | 2 | 1 B |
| 6x4 | anonymous public owners | reply | 383 | 3013 | 3026 | 4095 | 25 | 3 | 2 B |
| 6x4 | anonymous public owners | messages | 25 | 3006 | 3018 | 4088 | 25 | 3 | 26 B |
| 6x4 | anonymous public owners | message bytes | 824 | 3014 | 3026 | 4096 | 25 | 3 | 1 B |
| 6x4 | confidential entry | messages | 11 | 2997 | 3003 | 4079 | 25 | 3 | 17 B |
| 6x4 | confidential entry | message bytes | 357 | 3014 | 3020 | 4096 | 25 | 3 | 1 B |
| 6x4 | anonymous entry | reply | 141 | 3207 | 3221 | 4096 | 21 | 2 | 3 B |
| 6x4 | anonymous entry | messages | 3 | 3178 | 3191 | 4067 | 21 | 2 | 5 B |
| 6x4 | anonymous entry | message bytes | 97 | 3207 | 3220 | 4096 | 21 | 2 | 1 B |
| 6x8 | confidential signer | messages | 10 | 2995 | 3137 | 4077 | 25 | 3 | 15 B |
| 6x8 | confidential signer | message bytes | 325 | 3014 | 3156 | 4096 | 25 | 3 | 1 B |
| 6x8 | confidential in-proof | messages | 10 | 3091 | 3234 | 4076 | 24 | 2 | 14 B |
| 6x8 | confidential in-proof | message bytes | 326 | 3111 | 3254 | 4096 | 24 | 2 | 1 B |
| 6x8 | anonymous hidden | reply | none | - | - | - | - | - | 256 B |
| 6x8 | anonymous hidden | messages | none | - | - | - | - | - | 256 B |
| 6x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 291 B |
| 6x8 | anonymous public owners | reply | none | - | - | - | - | - | 385 B |
| 6x8 | anonymous public owners | messages | none | - | - | - | - | - | 385 B |
| 6x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 420 B |
| 6x8 | confidential entry | messages | none | - | - | - | - | - | 464 B |
| 6x8 | confidential entry | message bytes | none | - | - | - | - | - | 499 B |
| 6x8 | anonymous entry | reply | none | - | - | - | - | - | 1112 B |
| 6x8 | anonymous entry | messages | none | - | - | - | - | - | 1112 B |
| 6x8 | anonymous entry | message bytes | none | - | - | - | - | - | 1147 B |
| 8x2 | confidential signer | messages | 44 | 2931 | 2871 | 4079 | 27 | 3 | 17 B |
| 8x2 | confidential signer | message bytes | 1479 | 2948 | 2888 | 4096 | 27 | 3 | 1 B |
| 8x2 | confidential in-proof | messages | 44 | 3027 | 2968 | 4078 | 26 | 2 | 16 B |
| 8x2 | confidential in-proof | message bytes | 1480 | 3045 | 2986 | 4096 | 26 | 2 | 1 B |
| 8x2 | anonymous hidden | reply | 1577 | 3141 | 3121 | 4096 | 23 | 2 | 1 B |
| 8x2 | anonymous hidden | messages | 43 | 3124 | 3103 | 4079 | 23 | 2 | 17 B |
| 8x2 | anonymous hidden | message bytes | 1445 | 3141 | 3120 | 4096 | 23 | 2 | 1 B |
| 8x2 | anonymous public owners | reply | 1448 | 2948 | 2895 | 4096 | 27 | 3 | 1 B |
| 8x2 | anonymous public owners | messages | 39 | 2924 | 2870 | 4072 | 27 | 3 | 10 B |
| 8x2 | anonymous public owners | message bytes | 1316 | 2948 | 2894 | 4096 | 27 | 3 | 1 B |
| 8x2 | confidential entry | messages | 12 | 2939 | 2879 | 4087 | 27 | 3 | 25 B |
| 8x2 | confidential entry | message bytes | 383 | 2948 | 2888 | 4096 | 27 | 3 | 1 B |
| 8x2 | anonymous entry | reply | 449 | 3141 | 3089 | 4096 | 23 | 2 | 1 B |
| 8x2 | anonymous entry | messages | 10 | 3130 | 3077 | 4085 | 23 | 2 | 23 B |
| 8x2 | anonymous entry | message bytes | 317 | 3141 | 3088 | 4096 | 23 | 2 | 1 B |
| 8x4 | confidential signer | messages | 31 | 2917 | 2925 | 4065 | 27 | 3 | 3 B |
| 8x4 | confidential signer | message bytes | 1051 | 2948 | 2956 | 4096 | 27 | 3 | 1 B |
| 8x4 | confidential in-proof | messages | 31 | 3013 | 3022 | 4064 | 26 | 2 | 2 B |
| 8x4 | confidential in-proof | message bytes | 1052 | 3045 | 3054 | 4096 | 26 | 2 | 1 B |
| 8x4 | anonymous hidden | reply | 383 | 3141 | 3189 | 4096 | 23 | 2 | 3 B |
| 8x4 | anonymous hidden | messages | 25 | 3134 | 3181 | 4089 | 23 | 2 | 27 B |
| 8x4 | anonymous hidden | message bytes | 823 | 3141 | 3188 | 4096 | 23 | 2 | 1 B |
| 8x4 | anonymous public owners | reply | 340 | 2948 | 2963 | 4096 | 27 | 3 | 3 B |
| 8x4 | anonymous public owners | messages | 21 | 2934 | 2948 | 4082 | 27 | 3 | 20 B |
| 8x4 | anonymous public owners | message bytes | 694 | 2948 | 2962 | 4096 | 27 | 3 | 1 B |
| 8x4 | confidential entry | messages | none | - | - | - | - | - | 10 B |
| 8x4 | confidential entry | message bytes | none | - | - | - | - | - | 45 B |
| 8x4 | anonymous entry | reply | none | - | - | - | - | - | 270 B |
| 8x4 | anonymous entry | messages | none | - | - | - | - | - | 270 B |
| 8x4 | anonymous entry | message bytes | none | - | - | - | - | - | 305 B |
| 8x8 | confidential signer | messages | 6 | 2923 | 3067 | 4071 | 27 | 3 | 9 B |
| 8x8 | confidential signer | message bytes | 195 | 2948 | 3092 | 4096 | 27 | 3 | 1 B |
| 8x8 | confidential in-proof | messages | 6 | 3019 | 3164 | 4070 | 26 | 2 | 8 B |
| 8x8 | confidential in-proof | message bytes | 196 | 3045 | 3190 | 4096 | 26 | 2 | 1 B |
| 8x8 | anonymous hidden | reply | none | - | - | - | - | - | 386 B |
| 8x8 | anonymous hidden | messages | none | - | - | - | - | - | 386 B |
| 8x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 421 B |
| 8x8 | anonymous public owners | reply | none | - | - | - | - | - | 515 B |
| 8x8 | anonymous public owners | messages | none | - | - | - | - | - | 515 B |
| 8x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 550 B |
| 8x8 | confidential entry | messages | none | - | - | - | - | - | 866 B |
| 8x8 | confidential entry | message bytes | none | - | - | - | - | - | 901 B |
| 8x8 | anonymous entry | reply | none | - | - | - | - | - | 1514 B |
| 8x8 | anonymous entry | messages | none | - | - | - | - | - | 1514 B |
| 8x8 | anonymous entry | message bytes | none | - | - | - | - | - | 1549 B |
| 12x2 | confidential signer | messages | 36 | 2788 | 2731 | 4068 | 31 | 3 | 6 B |
| 12x2 | confidential signer | message bytes | 1218 | 2816 | 2759 | 4096 | 31 | 3 | 1 B |
| 12x2 | confidential in-proof | messages | 36 | 2884 | 2828 | 4067 | 30 | 2 | 5 B |
| 12x2 | confidential in-proof | message bytes | 1219 | 2913 | 2857 | 4096 | 30 | 2 | 1 B |
| 12x2 | anonymous hidden | reply | 1316 | 3009 | 2992 | 4096 | 27 | 2 | 1 B |
| 12x2 | anonymous hidden | messages | 35 | 2981 | 2963 | 4068 | 27 | 2 | 6 B |
| 12x2 | anonymous hidden | message bytes | 1184 | 3009 | 2991 | 4096 | 27 | 2 | 1 B |
| 12x2 | anonymous public owners | reply | 1187 | 2816 | 2766 | 4096 | 31 | 3 | 1 B |
| 12x2 | anonymous public owners | messages | 32 | 2815 | 2764 | 4095 | 31 | 3 | 33 B |
| 12x2 | anonymous public owners | message bytes | 1055 | 2816 | 2765 | 4096 | 31 | 3 | 1 B |
| 12x4 | confidential signer | messages | 24 | 2808 | 2819 | 4088 | 31 | 3 | 26 B |
| 12x4 | confidential signer | message bytes | 790 | 2816 | 2827 | 4096 | 31 | 3 | 1 B |
| 12x4 | confidential in-proof | messages | 24 | 2904 | 2916 | 4087 | 30 | 2 | 25 B |
| 12x4 | confidential in-proof | message bytes | 791 | 2913 | 2925 | 4096 | 30 | 2 | 1 B |
| 12x4 | anonymous hidden | reply | 296 | 3009 | 3060 | 4096 | 27 | 2 | 3 B |
| 12x4 | anonymous hidden | messages | 17 | 2991 | 3041 | 4078 | 27 | 2 | 16 B |
| 12x4 | anonymous hidden | message bytes | 562 | 3009 | 3059 | 4096 | 27 | 2 | 1 B |
| 12x4 | anonymous public owners | reply | 253 | 2816 | 2834 | 4096 | 31 | 3 | 3 B |
| 12x4 | anonymous public owners | messages | 13 | 2791 | 2808 | 4071 | 31 | 3 | 9 B |
| 12x4 | anonymous public owners | message bytes | 433 | 2816 | 2833 | 4096 | 31 | 3 | 1 B |
| 12x8 | confidential signer | messages | none | - | - | - | - | - | 31 B |
| 12x8 | confidential signer | message bytes | none | - | - | - | - | - | 66 B |
| 12x8 | confidential in-proof | messages | none | - | - | - | - | - | 30 B |
| 12x8 | confidential in-proof | message bytes | none | - | - | - | - | - | 65 B |
| 12x8 | anonymous hidden | reply | none | - | - | - | - | - | 647 B |
| 12x8 | anonymous hidden | messages | none | - | - | - | - | - | 647 B |
| 12x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 682 B |
| 12x8 | anonymous public owners | reply | none | - | - | - | - | - | 776 B |
| 12x8 | anonymous public owners | messages | none | - | - | - | - | - | 776 B |
| 12x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 811 B |
| 16x2 | confidential signer | messages | 29 | 2678 | 2625 | 4090 | 35 | 3 | 28 B |
| 16x2 | confidential signer | message bytes | 958 | 2684 | 2631 | 4096 | 35 | 3 | 1 B |
| 16x2 | confidential in-proof | messages | 29 | 2774 | 2722 | 4089 | 34 | 2 | 27 B |
| 16x2 | confidential in-proof | message bytes | 959 | 2781 | 2729 | 4096 | 34 | 2 | 1 B |
| 16x2 | anonymous hidden | reply | 1056 | 2877 | 2864 | 4096 | 31 | 2 | 1 B |
| 16x2 | anonymous hidden | messages | 28 | 2871 | 2857 | 4090 | 31 | 2 | 28 B |
| 16x2 | anonymous hidden | message bytes | 924 | 2877 | 2863 | 4096 | 31 | 2 | 1 B |
| 16x2 | anonymous public owners | reply | 927 | 2684 | 2638 | 4096 | 35 | 3 | 1 B |
| 16x2 | anonymous public owners | messages | 24 | 2671 | 2624 | 4083 | 35 | 3 | 21 B |
| 16x2 | anonymous public owners | message bytes | 795 | 2684 | 2637 | 4096 | 35 | 3 | 1 B |
| 16x4 | confidential signer | messages | 16 | 2664 | 2679 | 4076 | 35 | 3 | 14 B |
| 16x4 | confidential signer | message bytes | 530 | 2684 | 2699 | 4096 | 35 | 3 | 1 B |
| 16x4 | confidential in-proof | messages | 16 | 2760 | 2776 | 4075 | 34 | 2 | 13 B |
| 16x4 | confidential in-proof | message bytes | 531 | 2781 | 2797 | 4096 | 34 | 2 | 1 B |
| 16x4 | anonymous hidden | reply | 209 | 2876 | 2931 | 4095 | 31 | 2 | 2 B |
| 16x4 | anonymous hidden | messages | 9 | 2847 | 2901 | 4066 | 31 | 2 | 4 B |
| 16x4 | anonymous hidden | message bytes | 302 | 2877 | 2931 | 4096 | 31 | 2 | 1 B |
| 16x4 | anonymous public owners | reply | 166 | 2683 | 2705 | 4095 | 35 | 3 | 2 B |
| 16x4 | anonymous public owners | messages | 6 | 2681 | 2702 | 4093 | 35 | 3 | 31 B |
| 16x4 | anonymous public owners | message bytes | 173 | 2684 | 2705 | 4096 | 35 | 3 | 1 B |
| 16x8 | confidential signer | messages | none | - | - | - | - | - | 291 B |
| 16x8 | confidential signer | message bytes | none | - | - | - | - | - | 326 B |
| 16x8 | confidential in-proof | messages | none | - | - | - | - | - | 290 B |
| 16x8 | confidential in-proof | message bytes | none | - | - | - | - | - | 325 B |
| 16x8 | anonymous hidden | reply | none | - | - | - | - | - | 907 B |
| 16x8 | anonymous hidden | messages | none | - | - | - | - | - | 907 B |
| 16x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 942 B |
| 16x8 | anonymous public owners | reply | none | - | - | - | - | - | 1036 B |
| 16x8 | anonymous public owners | messages | none | - | - | - | - | - | 1036 B |
| 16x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 1071 B |
| 24x2 | confidential signer | messages | 13 | 2391 | 2345 | 4067 | 43 | 3 | 5 B |
| 24x2 | confidential signer | message bytes | 437 | 2420 | 2374 | 4096 | 43 | 3 | 1 B |
| 24x2 | confidential in-proof | messages | 13 | 2487 | 2442 | 4066 | 42 | 2 | 4 B |
| 24x2 | confidential in-proof | message bytes | 438 | 2517 | 2472 | 4096 | 42 | 2 | 1 B |
| 24x2 | anonymous hidden | reply | 535 | 2613 | 2607 | 4096 | 39 | 2 | 1 B |
| 24x2 | anonymous hidden | messages | 12 | 2584 | 2577 | 4067 | 39 | 2 | 5 B |
| 24x2 | anonymous hidden | message bytes | 403 | 2613 | 2606 | 4096 | 39 | 2 | 1 B |
| 24x2 | anonymous public owners | reply | 406 | 2420 | 2381 | 4096 | 43 | 3 | 1 B |
| 24x2 | anonymous public owners | messages | 9 | 2418 | 2378 | 4094 | 43 | 3 | 32 B |
| 24x2 | anonymous public owners | message bytes | 274 | 2420 | 2380 | 4096 | 43 | 3 | 1 B |
| 24x4 | confidential signer | messages | 1 | 2411 | 2433 | 4087 | 43 | 3 | 25 B |
| 24x4 | confidential signer | message bytes | 9 | 2420 | 2442 | 4096 | 43 | 3 | 1 B |
| 24x4 | confidential in-proof | messages | 1 | 2507 | 2530 | 4086 | 42 | 2 | 24 B |
| 24x4 | confidential in-proof | message bytes | 10 | 2517 | 2540 | 4096 | 42 | 2 | 1 B |
| 24x4 | anonymous hidden | reply | none | - | - | - | - | - | 184 B |
| 24x4 | anonymous hidden | messages | none | - | - | - | - | - | 184 B |
| 24x4 | anonymous hidden | message bytes | none | - | - | - | - | - | 219 B |
| 24x4 | anonymous public owners | reply | none | - | - | - | - | - | 313 B |
| 24x4 | anonymous public owners | messages | none | - | - | - | - | - | 313 B |
| 24x4 | anonymous public owners | message bytes | none | - | - | - | - | - | 348 B |
| 32x2 | confidential signer | messages | none | - | - | - | - | - | 49 B |
| 32x2 | confidential signer | message bytes | none | - | - | - | - | - | 84 B |
| 32x2 | confidential in-proof | messages | none | - | - | - | - | - | 48 B |
| 32x2 | confidential in-proof | message bytes | none | - | - | - | - | - | 83 B |
| 32x2 | anonymous hidden | reply | none | - | - | - | - | - | 83 B |
| 32x2 | anonymous hidden | messages | none | - | - | - | - | - | 83 B |
| 32x2 | anonymous hidden | message bytes | none | - | - | - | - | - | 118 B |
| 32x2 | anonymous public owners | reply | none | - | - | - | - | - | 212 B |
| 32x2 | anonymous public owners | messages | none | - | - | - | - | - | 212 B |
| 32x2 | anonymous public owners | message bytes | none | - | - | - | - | - | 247 B |
| 40x2 | confidential signer | messages | none | - | - | - | - | - | 570 B |
| 40x2 | confidential signer | message bytes | none | - | - | - | - | - | 605 B |
| 40x2 | confidential in-proof | messages | none | - | - | - | - | - | 569 B |
| 40x2 | confidential in-proof | message bytes | none | - | - | - | - | - | 604 B |
| 40x2 | anonymous hidden | reply | none | - | - | - | - | - | 604 B |
| 40x2 | anonymous hidden | messages | none | - | - | - | - | - | 604 B |
| 40x2 | anonymous hidden | message bytes | none | - | - | - | - | - | 639 B |
| 40x2 | anonymous public owners | reply | none | - | - | - | - | - | 733 B |
| 40x2 | anonymous public owners | messages | none | - | - | - | - | - | 733 B |
| 40x2 | anonymous public owners | message bytes | none | - | - | - | - | - | 768 B |
