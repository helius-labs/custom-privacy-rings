Measured with the frame encoders, SPP's account builders and the v1 compiler. Ceilings 4096 B, 64 accounts, 12 signatures.
min: tracking root, no co-signer, no leg, no fee, the payer an owner where it may be. full: own, curator and tracking roots, one co-signer, 4 SOL legs, a fee, an outside payer, a body on every slot and one exit. data: a data hash in every record. ring: a ring data hash in every non-exit record. program: the test program forwards the transfer under its proof and takes no leg, its PDA the one owner, and the co-signer approves at the top level. The owner run is the ceilings' bound under full, the key's rows bound it apart. A shape's every input and output slot is published.
Every row is one transaction. fits: the row clears the transaction and CPI ceilings. limit: what refuses a row, rail for a shape the rail has no SPP key for, else the excess in accounts, signatures, bytes or CPI data at that row's load. A missing catalog key has no encoded measurement.
| shape | rail | load | catalog key | bytes | accounts | signatures | owner run | frame | SPP CPI | fits | limit |
|---|---|---|---|---|---|---|---|---|---|---|---|
| 1x2 | confidential signer | min | yes | 1259 | 9 | 1 | - | 843 | 857 | yes |  |
| 1x2 | confidential signer | full | yes | 1918 | 19 | 3 | 1 | 1034 | 1006 | yes |  |
| 1x2 | confidential signer | full+spl | yes | 2286 | 26 | 3 | - | 1166 | 1010 | yes |  |
| 1x2 | confidential signer | full+data | yes | 2023 | 19 | 3 | - | 1139 | 1107 | yes |  |
| 1x2 | confidential signer | full+reply | yes | 1918 | 19 | 3 | - | 1034 | 1006 | yes |  |
| 1x2 | confidential signer | full+2tree | yes | 1956 | 20 | 3 | - | 1039 | 1010 | yes |  |
| 1x2 | confidential signer | full+data+ring | yes | 2055 | 19 | 3 | - | 1171 | 1139 | yes |  |
| 1x2 | confidential signer | full+data+program | yes | 1990 | 16 | 2 | - | 1103 | 1071 | yes |  |
| 1x2 | confidential signer | full+data+ring+program | yes | 2022 | 16 | 2 | - | 1135 | 1103 | yes |  |
| 1x2 | confidential in-proof | min | yes | 1354 | 9 | 1 | - | 939 | 954 | yes |  |
| 1x2 | confidential in-proof | full | yes | 1917 | 18 | 2 | 1 | 1130 | 1103 | yes |  |
| 1x2 | confidential in-proof | full+spl | yes | 2285 | 25 | 2 | - | 1262 | 1107 | yes |  |
| 1x2 | confidential in-proof | full+data | yes | 2022 | 18 | 2 | - | 1235 | 1204 | yes |  |
| 1x2 | confidential in-proof | full+reply | yes | 1917 | 18 | 2 | - | 1130 | 1103 | yes |  |
| 1x2 | confidential in-proof | full+2tree | yes | 1955 | 19 | 2 | - | 1135 | 1107 | yes |  |
| 1x2 | confidential in-proof | full+data+ring | yes | 2054 | 18 | 2 | - | 1267 | 1236 | yes |  |
| 1x2 | anonymous hidden | min | yes | 1322 | 9 | 1 | - | 907 | 954 | yes |  |
| 1x2 | anonymous hidden | full | yes | 1789 | 15 | 2 | 1 | 1098 | 1103 | yes |  |
| 1x2 | anonymous hidden | full+spl | yes | 2157 | 22 | 2 | - | 1230 | 1107 | yes |  |
| 1x2 | anonymous hidden | full+data | yes | 1894 | 15 | 2 | - | 1203 | 1204 | yes |  |
| 1x2 | anonymous hidden | full+reply | yes | 1888 | 15 | 2 | - | 1197 | 1200 | yes |  |
| 1x2 | anonymous hidden | full+2tree | yes | 1827 | 16 | 2 | - | 1103 | 1107 | yes |  |
| 1x2 | anonymous hidden | full+data+ring | yes | 1926 | 15 | 2 | - | 1235 | 1236 | yes |  |
| 1x2 | anonymous public owners | min | yes | 1259 | 9 | 1 | - | 843 | 857 | yes |  |
| 1x2 | anonymous public owners | full | yes | 1918 | 19 | 3 | 1 | 1034 | 1006 | yes |  |
| 1x2 | anonymous public owners | full+spl | yes | 2286 | 26 | 3 | - | 1166 | 1010 | yes |  |
| 1x2 | anonymous public owners | full+data | yes | 2023 | 19 | 3 | - | 1139 | 1107 | yes |  |
| 1x2 | anonymous public owners | full+reply | yes | 2017 | 19 | 3 | - | 1133 | 1103 | yes |  |
| 1x2 | anonymous public owners | full+2tree | yes | 1956 | 20 | 3 | - | 1039 | 1010 | yes |  |
| 1x2 | anonymous public owners | full+data+ring | yes | 2055 | 19 | 3 | - | 1171 | 1139 | yes |  |
| 1x2 | anonymous public owners | full+data+program | yes | 1990 | 16 | 2 | - | 1103 | 1071 | yes |  |
| 1x2 | anonymous public owners | full+data+ring+program | yes | 2022 | 16 | 2 | - | 1135 | 1103 | yes |  |
| 1x2 | confidential entry | min | yes | 1403 | 9 | 1 | - | 987 | 1001 | yes |  |
| 1x2 | confidential entry | full | yes | 2062 | 19 | 3 | 1 | 1178 | 1150 | yes |  |
| 1x2 | confidential entry | full+spl | yes | 2430 | 26 | 3 | - | 1310 | 1154 | yes |  |
| 1x2 | confidential entry | full+data | yes | 2167 | 19 | 3 | - | 1283 | 1251 | yes |  |
| 1x2 | confidential entry | full+reply | yes | 2062 | 19 | 3 | - | 1178 | 1150 | yes |  |
| 1x2 | confidential entry | full+2tree | yes | 2100 | 20 | 3 | - | 1183 | 1154 | yes |  |
| 1x2 | confidential entry | full+data+ring | yes | 2199 | 19 | 3 | - | 1315 | 1283 | yes |  |
| 1x2 | anonymous entry | min | yes | 1498 | 9 | 1 | - | 1083 | 1098 | yes |  |
| 1x2 | anonymous entry | full | yes | 1965 | 15 | 2 | 1 | 1274 | 1247 | yes |  |
| 1x2 | anonymous entry | full+spl | yes | 2333 | 22 | 2 | - | 1406 | 1251 | yes |  |
| 1x2 | anonymous entry | full+data | yes | 2070 | 15 | 2 | - | 1379 | 1348 | yes |  |
| 1x2 | anonymous entry | full+reply | yes | 2064 | 15 | 2 | - | 1373 | 1344 | yes |  |
| 1x2 | anonymous entry | full+2tree | yes | 2003 | 16 | 2 | - | 1279 | 1251 | yes |  |
| 1x2 | anonymous entry | full+data+ring | yes | 2102 | 15 | 2 | - | 1411 | 1380 | yes |  |
| 1x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x4 | confidential signer | min | yes | 1613 | 9 | 1 | - | 1197 | 1283 | yes |  |
| 1x4 | confidential signer | full | yes | 2272 | 19 | 3 | 1 | 1388 | 1432 | yes |  |
| 1x4 | confidential signer | full+spl | yes | 2640 | 26 | 3 | - | 1520 | 1436 | yes |  |
| 1x4 | confidential signer | full+data | yes | 2449 | 19 | 3 | - | 1565 | 1601 | yes |  |
| 1x4 | confidential signer | full+reply | yes | 2272 | 19 | 3 | - | 1388 | 1432 | yes |  |
| 1x4 | confidential signer | full+2tree | yes | 2310 | 20 | 3 | - | 1393 | 1436 | yes |  |
| 1x4 | confidential signer | full+data+ring | yes | 2545 | 19 | 3 | - | 1661 | 1697 | yes |  |
| 1x4 | confidential signer | full+data+program | yes | 2416 | 16 | 2 | - | 1529 | 1565 | yes |  |
| 1x4 | confidential signer | full+data+ring+program | yes | 2512 | 16 | 2 | - | 1625 | 1661 | yes |  |
| 1x4 | confidential in-proof | min | yes | 1708 | 9 | 1 | - | 1293 | 1380 | yes |  |
| 1x4 | confidential in-proof | full | yes | 2271 | 18 | 2 | 1 | 1484 | 1529 | yes |  |
| 1x4 | confidential in-proof | full+spl | yes | 2639 | 25 | 2 | - | 1616 | 1533 | yes |  |
| 1x4 | confidential in-proof | full+data | yes | 2448 | 18 | 2 | - | 1661 | 1698 | yes |  |
| 1x4 | confidential in-proof | full+reply | yes | 2271 | 18 | 2 | - | 1484 | 1529 | yes |  |
| 1x4 | confidential in-proof | full+2tree | yes | 2309 | 19 | 2 | - | 1489 | 1533 | yes |  |
| 1x4 | confidential in-proof | full+data+ring | yes | 2544 | 18 | 2 | - | 1757 | 1794 | yes |  |
| 1x4 | anonymous hidden | min | yes | 1676 | 9 | 1 | - | 1261 | 1380 | yes |  |
| 1x4 | anonymous hidden | full | yes | 2143 | 15 | 2 | 1 | 1452 | 1529 | yes |  |
| 1x4 | anonymous hidden | full+spl | yes | 2511 | 22 | 2 | - | 1584 | 1533 | yes |  |
| 1x4 | anonymous hidden | full+data | yes | 2320 | 15 | 2 | - | 1629 | 1698 | yes |  |
| 1x4 | anonymous hidden | full+reply | yes | 2436 | 15 | 2 | - | 1745 | 1820 | yes |  |
| 1x4 | anonymous hidden | full+2tree | yes | 2181 | 16 | 2 | - | 1457 | 1533 | yes |  |
| 1x4 | anonymous hidden | full+data+ring | yes | 2416 | 15 | 2 | - | 1725 | 1794 | yes |  |
| 1x4 | anonymous public owners | min | yes | 1613 | 9 | 1 | - | 1197 | 1283 | yes |  |
| 1x4 | anonymous public owners | full | yes | 2272 | 19 | 3 | 1 | 1388 | 1432 | yes |  |
| 1x4 | anonymous public owners | full+spl | yes | 2640 | 26 | 3 | - | 1520 | 1436 | yes |  |
| 1x4 | anonymous public owners | full+data | yes | 2449 | 19 | 3 | - | 1565 | 1601 | yes |  |
| 1x4 | anonymous public owners | full+reply | yes | 2565 | 19 | 3 | - | 1681 | 1723 | yes |  |
| 1x4 | anonymous public owners | full+2tree | yes | 2310 | 20 | 3 | - | 1393 | 1436 | yes |  |
| 1x4 | anonymous public owners | full+data+ring | yes | 2545 | 19 | 3 | - | 1661 | 1697 | yes |  |
| 1x4 | anonymous public owners | full+data+program | yes | 2416 | 16 | 2 | - | 1529 | 1565 | yes |  |
| 1x4 | anonymous public owners | full+data+ring+program | yes | 2512 | 16 | 2 | - | 1625 | 1661 | yes |  |
| 1x4 | confidential entry | min | yes | 1757 | 9 | 1 | - | 1341 | 1427 | yes |  |
| 1x4 | confidential entry | full | yes | 2416 | 19 | 3 | 1 | 1532 | 1576 | yes |  |
| 1x4 | confidential entry | full+spl | yes | 2784 | 26 | 3 | - | 1664 | 1580 | yes |  |
| 1x4 | confidential entry | full+data | yes | 2593 | 19 | 3 | - | 1709 | 1745 | yes |  |
| 1x4 | confidential entry | full+reply | yes | 2416 | 19 | 3 | - | 1532 | 1576 | yes |  |
| 1x4 | confidential entry | full+2tree | yes | 2454 | 20 | 3 | - | 1537 | 1580 | yes |  |
| 1x4 | confidential entry | full+data+ring | yes | 2689 | 19 | 3 | - | 1805 | 1841 | yes |  |
| 1x4 | anonymous entry | min | yes | 1852 | 9 | 1 | - | 1437 | 1524 | yes |  |
| 1x4 | anonymous entry | full | yes | 2319 | 15 | 2 | 1 | 1628 | 1673 | yes |  |
| 1x4 | anonymous entry | full+spl | yes | 2687 | 22 | 2 | - | 1760 | 1677 | yes |  |
| 1x4 | anonymous entry | full+data | yes | 2496 | 15 | 2 | - | 1805 | 1842 | yes |  |
| 1x4 | anonymous entry | full+reply | yes | 2612 | 15 | 2 | - | 1921 | 1964 | yes |  |
| 1x4 | anonymous entry | full+2tree | yes | 2357 | 16 | 2 | - | 1633 | 1677 | yes |  |
| 1x4 | anonymous entry | full+data+ring | yes | 2592 | 15 | 2 | - | 1901 | 1938 | yes |  |
| 1x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x8 | confidential signer | min | yes | 2321 | 9 | 1 | - | 1905 | 2135 | yes |  |
| 1x8 | confidential signer | full | yes | 2980 | 19 | 3 | 1 | 2096 | 2284 | yes |  |
| 1x8 | confidential signer | full+spl | yes | 3348 | 26 | 3 | - | 2228 | 2288 | yes |  |
| 1x8 | confidential signer | full+data | yes | 3301 | 19 | 3 | - | 2417 | 2589 | yes |  |
| 1x8 | confidential signer | full+reply | yes | 2980 | 19 | 3 | - | 2096 | 2284 | yes |  |
| 1x8 | confidential signer | full+2tree | yes | 3018 | 20 | 3 | - | 2101 | 2288 | yes |  |
| 1x8 | confidential signer | full+data+ring | yes | 3525 | 19 | 3 | - | 2641 | 2813 | yes |  |
| 1x8 | confidential signer | full+data+program | yes | 3268 | 16 | 2 | - | 2381 | 2553 | yes |  |
| 1x8 | confidential signer | full+data+ring+program | yes | 3492 | 16 | 2 | - | 2605 | 2777 | yes |  |
| 1x8 | confidential in-proof | min | yes | 2416 | 9 | 1 | - | 2001 | 2232 | yes |  |
| 1x8 | confidential in-proof | full | yes | 2979 | 18 | 2 | 1 | 2192 | 2381 | yes |  |
| 1x8 | confidential in-proof | full+spl | yes | 3347 | 25 | 2 | - | 2324 | 2385 | yes |  |
| 1x8 | confidential in-proof | full+data | yes | 3300 | 18 | 2 | - | 2513 | 2686 | yes |  |
| 1x8 | confidential in-proof | full+reply | yes | 2979 | 18 | 2 | - | 2192 | 2381 | yes |  |
| 1x8 | confidential in-proof | full+2tree | yes | 3017 | 19 | 2 | - | 2197 | 2385 | yes |  |
| 1x8 | confidential in-proof | full+data+ring | yes | 3524 | 18 | 2 | - | 2737 | 2910 | yes |  |
| 1x8 | anonymous hidden | min | yes | 2384 | 9 | 1 | - | 1969 | 2232 | yes |  |
| 1x8 | anonymous hidden | full | yes | 2851 | 15 | 2 | 1 | 2160 | 2381 | yes |  |
| 1x8 | anonymous hidden | full+spl | yes | 3219 | 22 | 2 | - | 2292 | 2385 | yes |  |
| 1x8 | anonymous hidden | full+data | yes | 3172 | 15 | 2 | - | 2481 | 2686 | yes |  |
| 1x8 | anonymous hidden | full+reply | yes | 3532 | 15 | 2 | - | 2841 | 3060 | yes |  |
| 1x8 | anonymous hidden | full+2tree | yes | 2889 | 16 | 2 | - | 2165 | 2385 | yes |  |
| 1x8 | anonymous hidden | full+data+ring | yes | 3396 | 15 | 2 | - | 2705 | 2910 | yes |  |
| 1x8 | anonymous public owners | min | yes | 2321 | 9 | 1 | - | 1905 | 2135 | yes |  |
| 1x8 | anonymous public owners | full | yes | 2980 | 19 | 3 | 1 | 2096 | 2284 | yes |  |
| 1x8 | anonymous public owners | full+spl | yes | 3348 | 26 | 3 | - | 2228 | 2288 | yes |  |
| 1x8 | anonymous public owners | full+data | yes | 3301 | 19 | 3 | - | 2417 | 2589 | yes |  |
| 1x8 | anonymous public owners | full+reply | yes | 3661 | 19 | 3 | - | 2777 | 2963 | yes |  |
| 1x8 | anonymous public owners | full+2tree | yes | 3018 | 20 | 3 | - | 2101 | 2288 | yes |  |
| 1x8 | anonymous public owners | full+data+ring | yes | 3525 | 19 | 3 | - | 2641 | 2813 | yes |  |
| 1x8 | anonymous public owners | full+data+program | yes | 3268 | 16 | 2 | - | 2381 | 2553 | yes |  |
| 1x8 | anonymous public owners | full+data+ring+program | yes | 3492 | 16 | 2 | - | 2605 | 2777 | yes |  |
| 1x8 | confidential entry | min | yes | 2465 | 9 | 1 | - | 2049 | 2279 | yes |  |
| 1x8 | confidential entry | full | yes | 3124 | 19 | 3 | 1 | 2240 | 2428 | yes |  |
| 1x8 | confidential entry | full+spl | yes | 3492 | 26 | 3 | - | 2372 | 2432 | yes |  |
| 1x8 | confidential entry | full+data | yes | 3445 | 19 | 3 | - | 2561 | 2733 | yes |  |
| 1x8 | confidential entry | full+reply | yes | 3124 | 19 | 3 | - | 2240 | 2428 | yes |  |
| 1x8 | confidential entry | full+2tree | yes | 3162 | 20 | 3 | - | 2245 | 2432 | yes |  |
| 1x8 | confidential entry | full+data+ring | yes | 3669 | 19 | 3 | - | 2785 | 2957 | yes |  |
| 1x8 | anonymous entry | min | yes | 2560 | 9 | 1 | - | 2145 | 2376 | yes |  |
| 1x8 | anonymous entry | full | yes | 3027 | 15 | 2 | 1 | 2336 | 2525 | yes |  |
| 1x8 | anonymous entry | full+spl | yes | 3395 | 22 | 2 | - | 2468 | 2529 | yes |  |
| 1x8 | anonymous entry | full+data | yes | 3348 | 15 | 2 | - | 2657 | 2830 | yes |  |
| 1x8 | anonymous entry | full+reply | yes | 3708 | 15 | 2 | - | 3017 | 3204 | yes |  |
| 1x8 | anonymous entry | full+2tree | yes | 3065 | 16 | 2 | - | 2341 | 2529 | yes |  |
| 1x8 | anonymous entry | full+data+ring | yes | 3572 | 15 | 2 | - | 2881 | 3054 | yes |  |
| 1x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x2 | confidential signer | min | yes | 1324 | 10 | 1 | - | 875 | 890 | yes |  |
| 2x2 | confidential signer | full | yes | 1983 | 20 | 3 | 2 | 1066 | 1039 | yes |  |
| 2x2 | confidential signer | full+spl | yes | 2351 | 27 | 3 | - | 1198 | 1043 | yes |  |
| 2x2 | confidential signer | full+data | yes | 2088 | 20 | 3 | - | 1171 | 1140 | yes |  |
| 2x2 | confidential signer | full+reply | yes | 1983 | 20 | 3 | - | 1066 | 1039 | yes |  |
| 2x2 | confidential signer | full+2tree | yes | 2021 | 21 | 3 | - | 1071 | 1043 | yes |  |
| 2x2 | confidential signer | full+data+ring | yes | 2120 | 20 | 3 | - | 1203 | 1172 | yes |  |
| 2x2 | confidential signer | full+data+program | yes | 2055 | 17 | 2 | - | 1135 | 1104 | yes |  |
| 2x2 | confidential signer | full+data+ring+program | yes | 2087 | 17 | 2 | - | 1167 | 1136 | yes |  |
| 2x2 | confidential in-proof | min | yes | 1419 | 10 | 1 | - | 971 | 987 | yes |  |
| 2x2 | confidential in-proof | full | yes | 1982 | 19 | 2 | 2 | 1162 | 1136 | yes |  |
| 2x2 | confidential in-proof | full+spl | yes | 2350 | 26 | 2 | - | 1294 | 1140 | yes |  |
| 2x2 | confidential in-proof | full+data | yes | 2087 | 19 | 2 | - | 1267 | 1237 | yes |  |
| 2x2 | confidential in-proof | full+reply | yes | 1982 | 19 | 2 | - | 1162 | 1136 | yes |  |
| 2x2 | confidential in-proof | full+2tree | yes | 2020 | 20 | 2 | - | 1167 | 1140 | yes |  |
| 2x2 | confidential in-proof | full+data+ring | yes | 2119 | 19 | 2 | - | 1299 | 1269 | yes |  |
| 2x2 | anonymous hidden | min | yes | 1387 | 10 | 1 | - | 939 | 987 | yes |  |
| 2x2 | anonymous hidden | full | yes | 1854 | 16 | 2 | 2 | 1130 | 1136 | yes |  |
| 2x2 | anonymous hidden | full+spl | yes | 2222 | 23 | 2 | - | 1262 | 1140 | yes |  |
| 2x2 | anonymous hidden | full+data | yes | 1959 | 16 | 2 | - | 1235 | 1237 | yes |  |
| 2x2 | anonymous hidden | full+reply | yes | 1953 | 16 | 2 | - | 1229 | 1233 | yes |  |
| 2x2 | anonymous hidden | full+2tree | yes | 1892 | 17 | 2 | - | 1135 | 1140 | yes |  |
| 2x2 | anonymous hidden | full+data+ring | yes | 1991 | 16 | 2 | - | 1267 | 1269 | yes |  |
| 2x2 | anonymous public owners | min | yes | 1324 | 10 | 1 | - | 875 | 890 | yes |  |
| 2x2 | anonymous public owners | full | yes | 1983 | 20 | 3 | 2 | 1066 | 1039 | yes |  |
| 2x2 | anonymous public owners | full+spl | yes | 2351 | 27 | 3 | - | 1198 | 1043 | yes |  |
| 2x2 | anonymous public owners | full+data | yes | 2088 | 20 | 3 | - | 1171 | 1140 | yes |  |
| 2x2 | anonymous public owners | full+reply | yes | 2082 | 20 | 3 | - | 1165 | 1136 | yes |  |
| 2x2 | anonymous public owners | full+2tree | yes | 2021 | 21 | 3 | - | 1071 | 1043 | yes |  |
| 2x2 | anonymous public owners | full+data+ring | yes | 2120 | 20 | 3 | - | 1203 | 1172 | yes |  |
| 2x2 | anonymous public owners | full+data+program | yes | 2055 | 17 | 2 | - | 1135 | 1104 | yes |  |
| 2x2 | anonymous public owners | full+data+ring+program | yes | 2087 | 17 | 2 | - | 1167 | 1136 | yes |  |
| 2x2 | confidential entry | min | yes | 1604 | 10 | 1 | - | 1155 | 1170 | yes |  |
| 2x2 | confidential entry | full | yes | 2263 | 20 | 3 | 2 | 1346 | 1319 | yes |  |
| 2x2 | confidential entry | full+spl | yes | 2631 | 27 | 3 | - | 1478 | 1323 | yes |  |
| 2x2 | confidential entry | full+data | yes | 2368 | 20 | 3 | - | 1451 | 1420 | yes |  |
| 2x2 | confidential entry | full+reply | yes | 2263 | 20 | 3 | - | 1346 | 1319 | yes |  |
| 2x2 | confidential entry | full+2tree | yes | 2301 | 21 | 3 | - | 1351 | 1323 | yes |  |
| 2x2 | confidential entry | full+data+ring | yes | 2400 | 20 | 3 | - | 1483 | 1452 | yes |  |
| 2x2 | anonymous entry | min | yes | 1699 | 10 | 1 | - | 1251 | 1267 | yes |  |
| 2x2 | anonymous entry | full | yes | 2166 | 16 | 2 | 2 | 1442 | 1416 | yes |  |
| 2x2 | anonymous entry | full+spl | yes | 2534 | 23 | 2 | - | 1574 | 1420 | yes |  |
| 2x2 | anonymous entry | full+data | yes | 2271 | 16 | 2 | - | 1547 | 1517 | yes |  |
| 2x2 | anonymous entry | full+reply | yes | 2265 | 16 | 2 | - | 1541 | 1513 | yes |  |
| 2x2 | anonymous entry | full+2tree | yes | 2204 | 17 | 2 | - | 1447 | 1420 | yes |  |
| 2x2 | anonymous entry | full+data+ring | yes | 2303 | 16 | 2 | - | 1579 | 1549 | yes |  |
| 2x2 | delegate | min | yes | 1324 | 10 | 1 | - | 875 | 890 | yes |  |
| 2x2 | delegate | full | yes | 1771 | 16 | 2 | - | 1050 | 1039 | yes |  |
| 2x2 | delegate | full+spl | yes | 2011 | 23 | 2 | - | 1054 | 1043 | yes |  |
| 2x2 | delegate | full+data | yes | 1876 | 16 | 2 | - | 1155 | 1140 | yes |  |
| 2x2 | delegate | full+reply | yes | 1771 | 16 | 2 | - | 1050 | 1039 | yes |  |
| 2x2 | delegate | full+2tree | yes | 1809 | 17 | 2 | - | 1055 | 1043 | yes |  |
| 2x2 | delegate | full+data+ring | yes | 1908 | 16 | 2 | - | 1187 | 1172 | yes |  |
| 2x2 | anonymous delegate | min | yes | 1324 | 10 | 1 | - | 875 | 890 | yes |  |
| 2x2 | anonymous delegate | full | yes | 1771 | 16 | 2 | - | 1050 | 1039 | yes |  |
| 2x2 | anonymous delegate | full+spl | yes | 2011 | 23 | 2 | - | 1054 | 1043 | yes |  |
| 2x2 | anonymous delegate | full+data | yes | 1876 | 16 | 2 | - | 1155 | 1140 | yes |  |
| 2x2 | anonymous delegate | full+reply | yes | 1870 | 16 | 2 | - | 1149 | 1136 | yes |  |
| 2x2 | anonymous delegate | full+2tree | yes | 1809 | 17 | 2 | - | 1055 | 1043 | yes |  |
| 2x2 | anonymous delegate | full+data+ring | yes | 1908 | 16 | 2 | - | 1187 | 1172 | yes |  |
| 2x4 | confidential signer | min | yes | 1678 | 10 | 1 | - | 1229 | 1316 | yes |  |
| 2x4 | confidential signer | full | yes | 2337 | 20 | 3 | 2 | 1420 | 1465 | yes |  |
| 2x4 | confidential signer | full+spl | yes | 2705 | 27 | 3 | - | 1552 | 1469 | yes |  |
| 2x4 | confidential signer | full+data | yes | 2514 | 20 | 3 | - | 1597 | 1634 | yes |  |
| 2x4 | confidential signer | full+reply | yes | 2337 | 20 | 3 | - | 1420 | 1465 | yes |  |
| 2x4 | confidential signer | full+2tree | yes | 2375 | 21 | 3 | - | 1425 | 1469 | yes |  |
| 2x4 | confidential signer | full+data+ring | yes | 2610 | 20 | 3 | - | 1693 | 1730 | yes |  |
| 2x4 | confidential signer | full+data+program | yes | 2481 | 17 | 2 | - | 1561 | 1598 | yes |  |
| 2x4 | confidential signer | full+data+ring+program | yes | 2577 | 17 | 2 | - | 1657 | 1694 | yes |  |
| 2x4 | confidential in-proof | min | yes | 1773 | 10 | 1 | - | 1325 | 1413 | yes |  |
| 2x4 | confidential in-proof | full | yes | 2336 | 19 | 2 | 2 | 1516 | 1562 | yes |  |
| 2x4 | confidential in-proof | full+spl | yes | 2704 | 26 | 2 | - | 1648 | 1566 | yes |  |
| 2x4 | confidential in-proof | full+data | yes | 2513 | 19 | 2 | - | 1693 | 1731 | yes |  |
| 2x4 | confidential in-proof | full+reply | yes | 2336 | 19 | 2 | - | 1516 | 1562 | yes |  |
| 2x4 | confidential in-proof | full+2tree | yes | 2374 | 20 | 2 | - | 1521 | 1566 | yes |  |
| 2x4 | confidential in-proof | full+data+ring | yes | 2609 | 19 | 2 | - | 1789 | 1827 | yes |  |
| 2x4 | anonymous hidden | min | yes | 1741 | 10 | 1 | - | 1293 | 1413 | yes |  |
| 2x4 | anonymous hidden | full | yes | 2208 | 16 | 2 | 2 | 1484 | 1562 | yes |  |
| 2x4 | anonymous hidden | full+spl | yes | 2576 | 23 | 2 | - | 1616 | 1566 | yes |  |
| 2x4 | anonymous hidden | full+data | yes | 2385 | 16 | 2 | - | 1661 | 1731 | yes |  |
| 2x4 | anonymous hidden | full+reply | yes | 2501 | 16 | 2 | - | 1777 | 1853 | yes |  |
| 2x4 | anonymous hidden | full+2tree | yes | 2246 | 17 | 2 | - | 1489 | 1566 | yes |  |
| 2x4 | anonymous hidden | full+data+ring | yes | 2481 | 16 | 2 | - | 1757 | 1827 | yes |  |
| 2x4 | anonymous public owners | min | yes | 1678 | 10 | 1 | - | 1229 | 1316 | yes |  |
| 2x4 | anonymous public owners | full | yes | 2337 | 20 | 3 | 2 | 1420 | 1465 | yes |  |
| 2x4 | anonymous public owners | full+spl | yes | 2705 | 27 | 3 | - | 1552 | 1469 | yes |  |
| 2x4 | anonymous public owners | full+data | yes | 2514 | 20 | 3 | - | 1597 | 1634 | yes |  |
| 2x4 | anonymous public owners | full+reply | yes | 2630 | 20 | 3 | - | 1713 | 1756 | yes |  |
| 2x4 | anonymous public owners | full+2tree | yes | 2375 | 21 | 3 | - | 1425 | 1469 | yes |  |
| 2x4 | anonymous public owners | full+data+ring | yes | 2610 | 20 | 3 | - | 1693 | 1730 | yes |  |
| 2x4 | anonymous public owners | full+data+program | yes | 2481 | 17 | 2 | - | 1561 | 1598 | yes |  |
| 2x4 | anonymous public owners | full+data+ring+program | yes | 2577 | 17 | 2 | - | 1657 | 1694 | yes |  |
| 2x4 | confidential entry | min | yes | 1958 | 10 | 1 | - | 1509 | 1596 | yes |  |
| 2x4 | confidential entry | full | yes | 2617 | 20 | 3 | 2 | 1700 | 1745 | yes |  |
| 2x4 | confidential entry | full+spl | yes | 2985 | 27 | 3 | - | 1832 | 1749 | yes |  |
| 2x4 | confidential entry | full+data | yes | 2794 | 20 | 3 | - | 1877 | 1914 | yes |  |
| 2x4 | confidential entry | full+reply | yes | 2617 | 20 | 3 | - | 1700 | 1745 | yes |  |
| 2x4 | confidential entry | full+2tree | yes | 2655 | 21 | 3 | - | 1705 | 1749 | yes |  |
| 2x4 | confidential entry | full+data+ring | yes | 2890 | 20 | 3 | - | 1973 | 2010 | yes |  |
| 2x4 | anonymous entry | min | yes | 2053 | 10 | 1 | - | 1605 | 1693 | yes |  |
| 2x4 | anonymous entry | full | yes | 2520 | 16 | 2 | 2 | 1796 | 1842 | yes |  |
| 2x4 | anonymous entry | full+spl | yes | 2888 | 23 | 2 | - | 1928 | 1846 | yes |  |
| 2x4 | anonymous entry | full+data | yes | 2697 | 16 | 2 | - | 1973 | 2011 | yes |  |
| 2x4 | anonymous entry | full+reply | yes | 2813 | 16 | 2 | - | 2089 | 2133 | yes |  |
| 2x4 | anonymous entry | full+2tree | yes | 2558 | 17 | 2 | - | 1801 | 1846 | yes |  |
| 2x4 | anonymous entry | full+data+ring | yes | 2793 | 16 | 2 | - | 2069 | 2107 | yes |  |
| 2x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x16 | confidential signer | min | yes | 3738 | 9 | 1 | - | 3322 | 3839 | yes |  |
| 1x16 | confidential signer | full | yes | 4397 | 19 | 3 | none | 3513 | 3988 | no | 301 B |
| 1x16 | confidential signer | full+spl | yes | 4765 | 26 | 3 | - | 3645 | 3992 | no | 669 B |
| 1x16 | confidential signer | full+data | yes | 5006 | 19 | 3 | - | 4122 | 4565 | no | 910 B |
| 1x16 | confidential signer | full+reply | yes | 4397 | 19 | 3 | - | 3513 | 3988 | no | 301 B |
| 1x16 | confidential signer | full+2tree | yes | 4435 | 20 | 3 | - | 3518 | 3992 | no | 339 B |
| 1x16 | confidential signer | full+data+ring | yes | 5486 | 19 | 3 | - | 4602 | 5045 | no | 1390 B |
| 1x16 | confidential signer | full+data+program | yes | 4973 | 16 | 2 | - | 4086 | 4529 | no | 877 B |
| 1x16 | confidential signer | full+data+ring+program | yes | 5453 | 16 | 2 | - | 4566 | 5009 | no | 1357 B |
| 1x16 | confidential in-proof | min | yes | 3833 | 9 | 1 | - | 3418 | 3936 | yes |  |
| 1x16 | confidential in-proof | full | yes | 4396 | 18 | 2 | none | 3609 | 4085 | no | 300 B |
| 1x16 | confidential in-proof | full+spl | yes | 4764 | 25 | 2 | - | 3741 | 4089 | no | 668 B |
| 1x16 | confidential in-proof | full+data | yes | 5005 | 18 | 2 | - | 4218 | 4662 | no | 909 B |
| 1x16 | confidential in-proof | full+reply | yes | 4396 | 18 | 2 | - | 3609 | 4085 | no | 300 B |
| 1x16 | confidential in-proof | full+2tree | yes | 4434 | 19 | 2 | - | 3614 | 4089 | no | 338 B |
| 1x16 | confidential in-proof | full+data+ring | yes | 5485 | 18 | 2 | - | 4698 | 5142 | no | 1389 B |
| 1x16 | anonymous hidden | min | yes | 3801 | 9 | 1 | - | 3386 | 3936 | yes |  |
| 1x16 | anonymous hidden | full | yes | 4268 | 15 | 2 | none | 3577 | 4085 | no | 172 B |
| 1x16 | anonymous hidden | full+spl | yes | 4636 | 22 | 2 | - | 3709 | 4089 | no | 540 B |
| 1x16 | anonymous hidden | full+data | yes | 4877 | 15 | 2 | - | 4186 | 4662 | no | 781 B |
| 1x16 | anonymous hidden | full+reply | yes | 5725 | 15 | 2 | - | 5034 | 5540 | no | 1629 B |
| 1x16 | anonymous hidden | full+2tree | yes | 4306 | 16 | 2 | - | 3582 | 4089 | no | 210 B |
| 1x16 | anonymous hidden | full+data+ring | yes | 5357 | 15 | 2 | - | 4666 | 5142 | no | 1261 B |
| 1x16 | anonymous public owners | min | yes | 3738 | 9 | 1 | - | 3322 | 3839 | yes |  |
| 1x16 | anonymous public owners | full | yes | 4397 | 19 | 3 | none | 3513 | 3988 | no | 301 B |
| 1x16 | anonymous public owners | full+spl | yes | 4765 | 26 | 3 | - | 3645 | 3992 | no | 669 B |
| 1x16 | anonymous public owners | full+data | yes | 5006 | 19 | 3 | - | 4122 | 4565 | no | 910 B |
| 1x16 | anonymous public owners | full+reply | yes | 5854 | 19 | 3 | - | 4970 | 5443 | no | 1758 B |
| 1x16 | anonymous public owners | full+2tree | yes | 4435 | 20 | 3 | - | 3518 | 3992 | no | 339 B |
| 1x16 | anonymous public owners | full+data+ring | yes | 5486 | 19 | 3 | - | 4602 | 5045 | no | 1390 B |
| 1x16 | anonymous public owners | full+data+program | yes | 4973 | 16 | 2 | - | 4086 | 4529 | no | 877 B |
| 1x16 | anonymous public owners | full+data+ring+program | yes | 5453 | 16 | 2 | - | 4566 | 5009 | no | 1357 B |
| 1x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 1x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 1x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x8 | confidential signer | min | yes | 2386 | 10 | 1 | - | 1937 | 2168 | yes |  |
| 2x8 | confidential signer | full | yes | 3045 | 20 | 3 | 2 | 2128 | 2317 | yes |  |
| 2x8 | confidential signer | full+spl | yes | 3413 | 27 | 3 | - | 2260 | 2321 | yes |  |
| 2x8 | confidential signer | full+data | yes | 3366 | 20 | 3 | - | 2449 | 2622 | yes |  |
| 2x8 | confidential signer | full+reply | yes | 3045 | 20 | 3 | - | 2128 | 2317 | yes |  |
| 2x8 | confidential signer | full+2tree | yes | 3083 | 21 | 3 | - | 2133 | 2321 | yes |  |
| 2x8 | confidential signer | full+data+ring | yes | 3590 | 20 | 3 | - | 2673 | 2846 | yes |  |
| 2x8 | confidential signer | full+data+program | yes | 3333 | 17 | 2 | - | 2413 | 2586 | yes |  |
| 2x8 | confidential signer | full+data+ring+program | yes | 3557 | 17 | 2 | - | 2637 | 2810 | yes |  |
| 2x8 | confidential in-proof | min | yes | 2481 | 10 | 1 | - | 2033 | 2265 | yes |  |
| 2x8 | confidential in-proof | full | yes | 3044 | 19 | 2 | 2 | 2224 | 2414 | yes |  |
| 2x8 | confidential in-proof | full+spl | yes | 3412 | 26 | 2 | - | 2356 | 2418 | yes |  |
| 2x8 | confidential in-proof | full+data | yes | 3365 | 19 | 2 | - | 2545 | 2719 | yes |  |
| 2x8 | confidential in-proof | full+reply | yes | 3044 | 19 | 2 | - | 2224 | 2414 | yes |  |
| 2x8 | confidential in-proof | full+2tree | yes | 3082 | 20 | 2 | - | 2229 | 2418 | yes |  |
| 2x8 | confidential in-proof | full+data+ring | yes | 3589 | 19 | 2 | - | 2769 | 2943 | yes |  |
| 2x8 | anonymous hidden | min | yes | 2449 | 10 | 1 | - | 2001 | 2265 | yes |  |
| 2x8 | anonymous hidden | full | yes | 2916 | 16 | 2 | 2 | 2192 | 2414 | yes |  |
| 2x8 | anonymous hidden | full+spl | yes | 3284 | 23 | 2 | - | 2324 | 2418 | yes |  |
| 2x8 | anonymous hidden | full+data | yes | 3237 | 16 | 2 | - | 2513 | 2719 | yes |  |
| 2x8 | anonymous hidden | full+reply | yes | 3597 | 16 | 2 | - | 2873 | 3093 | yes |  |
| 2x8 | anonymous hidden | full+2tree | yes | 2954 | 17 | 2 | - | 2197 | 2418 | yes |  |
| 2x8 | anonymous hidden | full+data+ring | yes | 3461 | 16 | 2 | - | 2737 | 2943 | yes |  |
| 2x8 | anonymous public owners | min | yes | 2386 | 10 | 1 | - | 1937 | 2168 | yes |  |
| 2x8 | anonymous public owners | full | yes | 3045 | 20 | 3 | 2 | 2128 | 2317 | yes |  |
| 2x8 | anonymous public owners | full+spl | yes | 3413 | 27 | 3 | - | 2260 | 2321 | yes |  |
| 2x8 | anonymous public owners | full+data | yes | 3366 | 20 | 3 | - | 2449 | 2622 | yes |  |
| 2x8 | anonymous public owners | full+reply | yes | 3726 | 20 | 3 | - | 2809 | 2996 | yes |  |
| 2x8 | anonymous public owners | full+2tree | yes | 3083 | 21 | 3 | - | 2133 | 2321 | yes |  |
| 2x8 | anonymous public owners | full+data+ring | yes | 3590 | 20 | 3 | - | 2673 | 2846 | yes |  |
| 2x8 | anonymous public owners | full+data+program | yes | 3333 | 17 | 2 | - | 2413 | 2586 | yes |  |
| 2x8 | anonymous public owners | full+data+ring+program | yes | 3557 | 17 | 2 | - | 2637 | 2810 | yes |  |
| 2x8 | confidential entry | min | yes | 2666 | 10 | 1 | - | 2217 | 2448 | yes |  |
| 2x8 | confidential entry | full | yes | 3325 | 20 | 3 | 2 | 2408 | 2597 | yes |  |
| 2x8 | confidential entry | full+spl | yes | 3693 | 27 | 3 | - | 2540 | 2601 | yes |  |
| 2x8 | confidential entry | full+data | yes | 3646 | 20 | 3 | - | 2729 | 2902 | yes |  |
| 2x8 | confidential entry | full+reply | yes | 3325 | 20 | 3 | - | 2408 | 2597 | yes |  |
| 2x8 | confidential entry | full+2tree | yes | 3363 | 21 | 3 | - | 2413 | 2601 | yes |  |
| 2x8 | confidential entry | full+data+ring | yes | 3870 | 20 | 3 | - | 2953 | 3126 | yes |  |
| 2x8 | anonymous entry | min | yes | 2761 | 10 | 1 | - | 2313 | 2545 | yes |  |
| 2x8 | anonymous entry | full | yes | 3228 | 16 | 2 | 2 | 2504 | 2694 | yes |  |
| 2x8 | anonymous entry | full+spl | yes | 3596 | 23 | 2 | - | 2636 | 2698 | yes |  |
| 2x8 | anonymous entry | full+data | yes | 3549 | 16 | 2 | - | 2825 | 2999 | yes |  |
| 2x8 | anonymous entry | full+reply | yes | 3909 | 16 | 2 | - | 3185 | 3373 | yes |  |
| 2x8 | anonymous entry | full+2tree | yes | 3266 | 17 | 2 | - | 2509 | 2698 | yes |  |
| 2x8 | anonymous entry | full+data+ring | yes | 3773 | 16 | 2 | - | 3049 | 3223 | yes |  |
| 2x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x2 | confidential signer | min | yes | 1389 | 11 | 1 | - | 907 | 923 | yes |  |
| 3x2 | confidential signer | full | yes | 2048 | 21 | 3 | 3 | 1098 | 1072 | yes |  |
| 3x2 | confidential signer | full+spl | yes | 2416 | 28 | 3 | - | 1230 | 1076 | yes |  |
| 3x2 | confidential signer | full+data | yes | 2153 | 21 | 3 | - | 1203 | 1173 | yes |  |
| 3x2 | confidential signer | full+reply | yes | 2048 | 21 | 3 | - | 1098 | 1072 | yes |  |
| 3x2 | confidential signer | full+2tree | yes | 2086 | 22 | 3 | - | 1103 | 1076 | yes |  |
| 3x2 | confidential signer | full+data+ring | yes | 2185 | 21 | 3 | - | 1235 | 1205 | yes |  |
| 3x2 | confidential signer | full+data+program | yes | 2120 | 18 | 2 | - | 1167 | 1137 | yes |  |
| 3x2 | confidential signer | full+data+ring+program | yes | 2152 | 18 | 2 | - | 1199 | 1169 | yes |  |
| 3x2 | confidential in-proof | min | yes | 1484 | 11 | 1 | - | 1003 | 1020 | yes |  |
| 3x2 | confidential in-proof | full | yes | 2047 | 20 | 2 | 3 | 1194 | 1169 | yes |  |
| 3x2 | confidential in-proof | full+spl | yes | 2415 | 27 | 2 | - | 1326 | 1173 | yes |  |
| 3x2 | confidential in-proof | full+data | yes | 2152 | 20 | 2 | - | 1299 | 1270 | yes |  |
| 3x2 | confidential in-proof | full+reply | yes | 2047 | 20 | 2 | - | 1194 | 1169 | yes |  |
| 3x2 | confidential in-proof | full+2tree | yes | 2085 | 21 | 2 | - | 1199 | 1173 | yes |  |
| 3x2 | confidential in-proof | full+data+ring | yes | 2184 | 20 | 2 | - | 1331 | 1302 | yes |  |
| 3x2 | anonymous hidden | min | yes | 1452 | 11 | 1 | - | 971 | 1020 | yes |  |
| 3x2 | anonymous hidden | full | yes | 1919 | 17 | 2 | 3 | 1162 | 1169 | yes |  |
| 3x2 | anonymous hidden | full+spl | yes | 2287 | 24 | 2 | - | 1294 | 1173 | yes |  |
| 3x2 | anonymous hidden | full+data | yes | 2024 | 17 | 2 | - | 1267 | 1270 | yes |  |
| 3x2 | anonymous hidden | full+reply | yes | 2018 | 17 | 2 | - | 1261 | 1266 | yes |  |
| 3x2 | anonymous hidden | full+2tree | yes | 1957 | 18 | 2 | - | 1167 | 1173 | yes |  |
| 3x2 | anonymous hidden | full+data+ring | yes | 2056 | 17 | 2 | - | 1299 | 1302 | yes |  |
| 3x2 | anonymous public owners | min | yes | 1389 | 11 | 1 | - | 907 | 923 | yes |  |
| 3x2 | anonymous public owners | full | yes | 2048 | 21 | 3 | 3 | 1098 | 1072 | yes |  |
| 3x2 | anonymous public owners | full+spl | yes | 2416 | 28 | 3 | - | 1230 | 1076 | yes |  |
| 3x2 | anonymous public owners | full+data | yes | 2153 | 21 | 3 | - | 1203 | 1173 | yes |  |
| 3x2 | anonymous public owners | full+reply | yes | 2147 | 21 | 3 | - | 1197 | 1169 | yes |  |
| 3x2 | anonymous public owners | full+2tree | yes | 2086 | 22 | 3 | - | 1103 | 1076 | yes |  |
| 3x2 | anonymous public owners | full+data+ring | yes | 2185 | 21 | 3 | - | 1235 | 1205 | yes |  |
| 3x2 | anonymous public owners | full+data+program | yes | 2120 | 18 | 2 | - | 1167 | 1137 | yes |  |
| 3x2 | anonymous public owners | full+data+ring+program | yes | 2152 | 18 | 2 | - | 1199 | 1169 | yes |  |
| 3x2 | confidential entry | min | yes | 1805 | 11 | 1 | - | 1323 | 1339 | yes |  |
| 3x2 | confidential entry | full | yes | 2464 | 21 | 3 | 3 | 1514 | 1488 | yes |  |
| 3x2 | confidential entry | full+spl | yes | 2832 | 28 | 3 | - | 1646 | 1492 | yes |  |
| 3x2 | confidential entry | full+data | yes | 2569 | 21 | 3 | - | 1619 | 1589 | yes |  |
| 3x2 | confidential entry | full+reply | yes | 2464 | 21 | 3 | - | 1514 | 1488 | yes |  |
| 3x2 | confidential entry | full+2tree | yes | 2502 | 22 | 3 | - | 1519 | 1492 | yes |  |
| 3x2 | confidential entry | full+data+ring | yes | 2601 | 21 | 3 | - | 1651 | 1621 | yes |  |
| 3x2 | anonymous entry | min | yes | 1900 | 11 | 1 | - | 1419 | 1436 | yes |  |
| 3x2 | anonymous entry | full | yes | 2367 | 17 | 2 | 3 | 1610 | 1585 | yes |  |
| 3x2 | anonymous entry | full+spl | yes | 2735 | 24 | 2 | - | 1742 | 1589 | yes |  |
| 3x2 | anonymous entry | full+data | yes | 2472 | 17 | 2 | - | 1715 | 1686 | yes |  |
| 3x2 | anonymous entry | full+reply | yes | 2466 | 17 | 2 | - | 1709 | 1682 | yes |  |
| 3x2 | anonymous entry | full+2tree | yes | 2405 | 18 | 2 | - | 1615 | 1589 | yes |  |
| 3x2 | anonymous entry | full+data+ring | yes | 2504 | 17 | 2 | - | 1747 | 1718 | yes |  |
| 3x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x4 | confidential signer | min | yes | 1743 | 11 | 1 | - | 1261 | 1349 | yes |  |
| 3x4 | confidential signer | full | yes | 2402 | 21 | 3 | 3 | 1452 | 1498 | yes |  |
| 3x4 | confidential signer | full+spl | yes | 2770 | 28 | 3 | - | 1584 | 1502 | yes |  |
| 3x4 | confidential signer | full+data | yes | 2579 | 21 | 3 | - | 1629 | 1667 | yes |  |
| 3x4 | confidential signer | full+reply | yes | 2402 | 21 | 3 | - | 1452 | 1498 | yes |  |
| 3x4 | confidential signer | full+2tree | yes | 2440 | 22 | 3 | - | 1457 | 1502 | yes |  |
| 3x4 | confidential signer | full+data+ring | yes | 2675 | 21 | 3 | - | 1725 | 1763 | yes |  |
| 3x4 | confidential signer | full+data+program | yes | 2546 | 18 | 2 | - | 1593 | 1631 | yes |  |
| 3x4 | confidential signer | full+data+ring+program | yes | 2642 | 18 | 2 | - | 1689 | 1727 | yes |  |
| 3x4 | confidential in-proof | min | yes | 1838 | 11 | 1 | - | 1357 | 1446 | yes |  |
| 3x4 | confidential in-proof | full | yes | 2401 | 20 | 2 | 3 | 1548 | 1595 | yes |  |
| 3x4 | confidential in-proof | full+spl | yes | 2769 | 27 | 2 | - | 1680 | 1599 | yes |  |
| 3x4 | confidential in-proof | full+data | yes | 2578 | 20 | 2 | - | 1725 | 1764 | yes |  |
| 3x4 | confidential in-proof | full+reply | yes | 2401 | 20 | 2 | - | 1548 | 1595 | yes |  |
| 3x4 | confidential in-proof | full+2tree | yes | 2439 | 21 | 2 | - | 1553 | 1599 | yes |  |
| 3x4 | confidential in-proof | full+data+ring | yes | 2674 | 20 | 2 | - | 1821 | 1860 | yes |  |
| 3x4 | anonymous hidden | min | yes | 1806 | 11 | 1 | - | 1325 | 1446 | yes |  |
| 3x4 | anonymous hidden | full | yes | 2273 | 17 | 2 | 3 | 1516 | 1595 | yes |  |
| 3x4 | anonymous hidden | full+spl | yes | 2641 | 24 | 2 | - | 1648 | 1599 | yes |  |
| 3x4 | anonymous hidden | full+data | yes | 2450 | 17 | 2 | - | 1693 | 1764 | yes |  |
| 3x4 | anonymous hidden | full+reply | yes | 2566 | 17 | 2 | - | 1809 | 1886 | yes |  |
| 3x4 | anonymous hidden | full+2tree | yes | 2311 | 18 | 2 | - | 1521 | 1599 | yes |  |
| 3x4 | anonymous hidden | full+data+ring | yes | 2546 | 17 | 2 | - | 1789 | 1860 | yes |  |
| 3x4 | anonymous public owners | min | yes | 1743 | 11 | 1 | - | 1261 | 1349 | yes |  |
| 3x4 | anonymous public owners | full | yes | 2402 | 21 | 3 | 3 | 1452 | 1498 | yes |  |
| 3x4 | anonymous public owners | full+spl | yes | 2770 | 28 | 3 | - | 1584 | 1502 | yes |  |
| 3x4 | anonymous public owners | full+data | yes | 2579 | 21 | 3 | - | 1629 | 1667 | yes |  |
| 3x4 | anonymous public owners | full+reply | yes | 2695 | 21 | 3 | - | 1745 | 1789 | yes |  |
| 3x4 | anonymous public owners | full+2tree | yes | 2440 | 22 | 3 | - | 1457 | 1502 | yes |  |
| 3x4 | anonymous public owners | full+data+ring | yes | 2675 | 21 | 3 | - | 1725 | 1763 | yes |  |
| 3x4 | anonymous public owners | full+data+program | yes | 2546 | 18 | 2 | - | 1593 | 1631 | yes |  |
| 3x4 | anonymous public owners | full+data+ring+program | yes | 2642 | 18 | 2 | - | 1689 | 1727 | yes |  |
| 3x4 | confidential entry | min | yes | 2159 | 11 | 1 | - | 1677 | 1765 | yes |  |
| 3x4 | confidential entry | full | yes | 2818 | 21 | 3 | 3 | 1868 | 1914 | yes |  |
| 3x4 | confidential entry | full+spl | yes | 3186 | 28 | 3 | - | 2000 | 1918 | yes |  |
| 3x4 | confidential entry | full+data | yes | 2995 | 21 | 3 | - | 2045 | 2083 | yes |  |
| 3x4 | confidential entry | full+reply | yes | 2818 | 21 | 3 | - | 1868 | 1914 | yes |  |
| 3x4 | confidential entry | full+2tree | yes | 2856 | 22 | 3 | - | 1873 | 1918 | yes |  |
| 3x4 | confidential entry | full+data+ring | yes | 3091 | 21 | 3 | - | 2141 | 2179 | yes |  |
| 3x4 | anonymous entry | min | yes | 2254 | 11 | 1 | - | 1773 | 1862 | yes |  |
| 3x4 | anonymous entry | full | yes | 2721 | 17 | 2 | 3 | 1964 | 2011 | yes |  |
| 3x4 | anonymous entry | full+spl | yes | 3089 | 24 | 2 | - | 2096 | 2015 | yes |  |
| 3x4 | anonymous entry | full+data | yes | 2898 | 17 | 2 | - | 2141 | 2180 | yes |  |
| 3x4 | anonymous entry | full+reply | yes | 3014 | 17 | 2 | - | 2257 | 2302 | yes |  |
| 3x4 | anonymous entry | full+2tree | yes | 2759 | 18 | 2 | - | 1969 | 2015 | yes |  |
| 3x4 | anonymous entry | full+data+ring | yes | 2994 | 17 | 2 | - | 2237 | 2276 | yes |  |
| 3x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x16 | confidential signer | min | yes | 3803 | 10 | 1 | - | 3354 | 3872 | yes |  |
| 2x16 | confidential signer | full | yes | 4462 | 20 | 3 | none | 3545 | 4021 | no | 366 B |
| 2x16 | confidential signer | full+spl | yes | 4830 | 27 | 3 | - | 3677 | 4025 | no | 734 B |
| 2x16 | confidential signer | full+data | yes | 5071 | 20 | 3 | - | 4154 | 4598 | no | 975 B |
| 2x16 | confidential signer | full+reply | yes | 4462 | 20 | 3 | - | 3545 | 4021 | no | 366 B |
| 2x16 | confidential signer | full+2tree | yes | 4500 | 21 | 3 | - | 3550 | 4025 | no | 404 B |
| 2x16 | confidential signer | full+data+ring | yes | 5551 | 20 | 3 | - | 4634 | 5078 | no | 1455 B |
| 2x16 | confidential signer | full+data+program | yes | 5038 | 17 | 2 | - | 4118 | 4562 | no | 942 B |
| 2x16 | confidential signer | full+data+ring+program | yes | 5518 | 17 | 2 | - | 4598 | 5042 | no | 1422 B |
| 2x16 | confidential in-proof | min | yes | 3898 | 10 | 1 | - | 3450 | 3969 | yes |  |
| 2x16 | confidential in-proof | full | yes | 4461 | 19 | 2 | none | 3641 | 4118 | no | 365 B |
| 2x16 | confidential in-proof | full+spl | yes | 4829 | 26 | 2 | - | 3773 | 4122 | no | 733 B |
| 2x16 | confidential in-proof | full+data | yes | 5070 | 19 | 2 | - | 4250 | 4695 | no | 974 B |
| 2x16 | confidential in-proof | full+reply | yes | 4461 | 19 | 2 | - | 3641 | 4118 | no | 365 B |
| 2x16 | confidential in-proof | full+2tree | yes | 4499 | 20 | 2 | - | 3646 | 4122 | no | 403 B |
| 2x16 | confidential in-proof | full+data+ring | yes | 5550 | 19 | 2 | - | 4730 | 5175 | no | 1454 B |
| 2x16 | anonymous hidden | min | yes | 3866 | 10 | 1 | - | 3418 | 3969 | yes |  |
| 2x16 | anonymous hidden | full | yes | 4333 | 16 | 2 | none | 3609 | 4118 | no | 237 B |
| 2x16 | anonymous hidden | full+spl | yes | 4701 | 23 | 2 | - | 3741 | 4122 | no | 605 B |
| 2x16 | anonymous hidden | full+data | yes | 4942 | 16 | 2 | - | 4218 | 4695 | no | 846 B |
| 2x16 | anonymous hidden | full+reply | yes | 5790 | 16 | 2 | - | 5066 | 5573 | no | 1694 B |
| 2x16 | anonymous hidden | full+2tree | yes | 4371 | 17 | 2 | - | 3614 | 4122 | no | 275 B |
| 2x16 | anonymous hidden | full+data+ring | yes | 5422 | 16 | 2 | - | 4698 | 5175 | no | 1326 B |
| 2x16 | anonymous public owners | min | yes | 3803 | 10 | 1 | - | 3354 | 3872 | yes |  |
| 2x16 | anonymous public owners | full | yes | 4462 | 20 | 3 | none | 3545 | 4021 | no | 366 B |
| 2x16 | anonymous public owners | full+spl | yes | 4830 | 27 | 3 | - | 3677 | 4025 | no | 734 B |
| 2x16 | anonymous public owners | full+data | yes | 5071 | 20 | 3 | - | 4154 | 4598 | no | 975 B |
| 2x16 | anonymous public owners | full+reply | yes | 5919 | 20 | 3 | - | 5002 | 5476 | no | 1823 B |
| 2x16 | anonymous public owners | full+2tree | yes | 4500 | 21 | 3 | - | 3550 | 4025 | no | 404 B |
| 2x16 | anonymous public owners | full+data+ring | yes | 5551 | 20 | 3 | - | 4634 | 5078 | no | 1455 B |
| 2x16 | anonymous public owners | full+data+program | yes | 5038 | 17 | 2 | - | 4118 | 4562 | no | 942 B |
| 2x16 | anonymous public owners | full+data+ring+program | yes | 5518 | 17 | 2 | - | 4598 | 5042 | no | 1422 B |
| 2x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 2x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 2x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x8 | confidential signer | min | yes | 2451 | 11 | 1 | - | 1969 | 2201 | yes |  |
| 3x8 | confidential signer | full | yes | 3110 | 21 | 3 | 3 | 2160 | 2350 | yes |  |
| 3x8 | confidential signer | full+spl | yes | 3478 | 28 | 3 | - | 2292 | 2354 | yes |  |
| 3x8 | confidential signer | full+data | yes | 3431 | 21 | 3 | - | 2481 | 2655 | yes |  |
| 3x8 | confidential signer | full+reply | yes | 3110 | 21 | 3 | - | 2160 | 2350 | yes |  |
| 3x8 | confidential signer | full+2tree | yes | 3148 | 22 | 3 | - | 2165 | 2354 | yes |  |
| 3x8 | confidential signer | full+data+ring | yes | 3655 | 21 | 3 | - | 2705 | 2879 | yes |  |
| 3x8 | confidential signer | full+data+program | yes | 3398 | 18 | 2 | - | 2445 | 2619 | yes |  |
| 3x8 | confidential signer | full+data+ring+program | yes | 3622 | 18 | 2 | - | 2669 | 2843 | yes |  |
| 3x8 | confidential in-proof | min | yes | 2546 | 11 | 1 | - | 2065 | 2298 | yes |  |
| 3x8 | confidential in-proof | full | yes | 3109 | 20 | 2 | 3 | 2256 | 2447 | yes |  |
| 3x8 | confidential in-proof | full+spl | yes | 3477 | 27 | 2 | - | 2388 | 2451 | yes |  |
| 3x8 | confidential in-proof | full+data | yes | 3430 | 20 | 2 | - | 2577 | 2752 | yes |  |
| 3x8 | confidential in-proof | full+reply | yes | 3109 | 20 | 2 | - | 2256 | 2447 | yes |  |
| 3x8 | confidential in-proof | full+2tree | yes | 3147 | 21 | 2 | - | 2261 | 2451 | yes |  |
| 3x8 | confidential in-proof | full+data+ring | yes | 3654 | 20 | 2 | - | 2801 | 2976 | yes |  |
| 3x8 | anonymous hidden | min | yes | 2514 | 11 | 1 | - | 2033 | 2298 | yes |  |
| 3x8 | anonymous hidden | full | yes | 2981 | 17 | 2 | 3 | 2224 | 2447 | yes |  |
| 3x8 | anonymous hidden | full+spl | yes | 3349 | 24 | 2 | - | 2356 | 2451 | yes |  |
| 3x8 | anonymous hidden | full+data | yes | 3302 | 17 | 2 | - | 2545 | 2752 | yes |  |
| 3x8 | anonymous hidden | full+reply | yes | 3662 | 17 | 2 | - | 2905 | 3126 | yes |  |
| 3x8 | anonymous hidden | full+2tree | yes | 3019 | 18 | 2 | - | 2229 | 2451 | yes |  |
| 3x8 | anonymous hidden | full+data+ring | yes | 3526 | 17 | 2 | - | 2769 | 2976 | yes |  |
| 3x8 | anonymous public owners | min | yes | 2451 | 11 | 1 | - | 1969 | 2201 | yes |  |
| 3x8 | anonymous public owners | full | yes | 3110 | 21 | 3 | 3 | 2160 | 2350 | yes |  |
| 3x8 | anonymous public owners | full+spl | yes | 3478 | 28 | 3 | - | 2292 | 2354 | yes |  |
| 3x8 | anonymous public owners | full+data | yes | 3431 | 21 | 3 | - | 2481 | 2655 | yes |  |
| 3x8 | anonymous public owners | full+reply | yes | 3791 | 21 | 3 | - | 2841 | 3029 | yes |  |
| 3x8 | anonymous public owners | full+2tree | yes | 3148 | 22 | 3 | - | 2165 | 2354 | yes |  |
| 3x8 | anonymous public owners | full+data+ring | yes | 3655 | 21 | 3 | - | 2705 | 2879 | yes |  |
| 3x8 | anonymous public owners | full+data+program | yes | 3398 | 18 | 2 | - | 2445 | 2619 | yes |  |
| 3x8 | anonymous public owners | full+data+ring+program | yes | 3622 | 18 | 2 | - | 2669 | 2843 | yes |  |
| 3x8 | confidential entry | min | yes | 2867 | 11 | 1 | - | 2385 | 2617 | yes |  |
| 3x8 | confidential entry | full | yes | 3526 | 21 | 3 | 3 | 2576 | 2766 | yes |  |
| 3x8 | confidential entry | full+spl | yes | 3894 | 28 | 3 | - | 2708 | 2770 | yes |  |
| 3x8 | confidential entry | full+data | yes | 3847 | 21 | 3 | - | 2897 | 3071 | yes |  |
| 3x8 | confidential entry | full+reply | yes | 3526 | 21 | 3 | - | 2576 | 2766 | yes |  |
| 3x8 | confidential entry | full+2tree | yes | 3564 | 22 | 3 | - | 2581 | 2770 | yes |  |
| 3x8 | confidential entry | full+data+ring | yes | 4071 | 21 | 3 | - | 3121 | 3295 | yes |  |
| 3x8 | anonymous entry | min | yes | 2962 | 11 | 1 | - | 2481 | 2714 | yes |  |
| 3x8 | anonymous entry | full | yes | 3429 | 17 | 2 | 3 | 2672 | 2863 | yes |  |
| 3x8 | anonymous entry | full+spl | yes | 3797 | 24 | 2 | - | 2804 | 2867 | yes |  |
| 3x8 | anonymous entry | full+data | yes | 3750 | 17 | 2 | - | 2993 | 3168 | yes |  |
| 3x8 | anonymous entry | full+reply | yes | 4110 | 17 | 2 | - | 3353 | 3542 | no | 14 B |
| 3x8 | anonymous entry | full+2tree | yes | 3467 | 18 | 2 | - | 2677 | 2867 | yes |  |
| 3x8 | anonymous entry | full+data+ring | yes | 3974 | 17 | 2 | - | 3217 | 3392 | yes |  |
| 3x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 3x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x2 | confidential signer | min | yes | 1454 | 12 | 1 | - | 939 | 956 | yes |  |
| 4x2 | confidential signer | full | yes | 2113 | 22 | 3 | 4 | 1130 | 1105 | yes |  |
| 4x2 | confidential signer | full+spl | yes | 2481 | 29 | 3 | - | 1262 | 1109 | yes |  |
| 4x2 | confidential signer | full+data | yes | 2218 | 22 | 3 | - | 1235 | 1206 | yes |  |
| 4x2 | confidential signer | full+reply | yes | 2113 | 22 | 3 | - | 1130 | 1105 | yes |  |
| 4x2 | confidential signer | full+2tree | yes | 2151 | 23 | 3 | - | 1135 | 1109 | yes |  |
| 4x2 | confidential signer | full+data+ring | yes | 2250 | 22 | 3 | - | 1267 | 1238 | yes |  |
| 4x2 | confidential signer | full+data+program | yes | 2185 | 19 | 2 | - | 1199 | 1170 | yes |  |
| 4x2 | confidential signer | full+data+ring+program | yes | 2217 | 19 | 2 | - | 1231 | 1202 | yes |  |
| 4x2 | confidential in-proof | min | yes | 1549 | 12 | 1 | - | 1035 | 1053 | yes |  |
| 4x2 | confidential in-proof | full | yes | 2112 | 21 | 2 | 4 | 1226 | 1202 | yes |  |
| 4x2 | confidential in-proof | full+spl | yes | 2480 | 28 | 2 | - | 1358 | 1206 | yes |  |
| 4x2 | confidential in-proof | full+data | yes | 2217 | 21 | 2 | - | 1331 | 1303 | yes |  |
| 4x2 | confidential in-proof | full+reply | yes | 2112 | 21 | 2 | - | 1226 | 1202 | yes |  |
| 4x2 | confidential in-proof | full+2tree | yes | 2150 | 22 | 2 | - | 1231 | 1206 | yes |  |
| 4x2 | confidential in-proof | full+data+ring | yes | 2249 | 21 | 2 | - | 1363 | 1335 | yes |  |
| 4x2 | anonymous hidden | min | yes | 1517 | 12 | 1 | - | 1003 | 1053 | yes |  |
| 4x2 | anonymous hidden | full | yes | 1984 | 18 | 2 | 4 | 1194 | 1202 | yes |  |
| 4x2 | anonymous hidden | full+spl | yes | 2352 | 25 | 2 | - | 1326 | 1206 | yes |  |
| 4x2 | anonymous hidden | full+data | yes | 2089 | 18 | 2 | - | 1299 | 1303 | yes |  |
| 4x2 | anonymous hidden | full+reply | yes | 2083 | 18 | 2 | - | 1293 | 1299 | yes |  |
| 4x2 | anonymous hidden | full+2tree | yes | 2022 | 19 | 2 | - | 1199 | 1206 | yes |  |
| 4x2 | anonymous hidden | full+data+ring | yes | 2121 | 18 | 2 | - | 1331 | 1335 | yes |  |
| 4x2 | anonymous public owners | min | yes | 1454 | 12 | 1 | - | 939 | 956 | yes |  |
| 4x2 | anonymous public owners | full | yes | 2113 | 22 | 3 | 4 | 1130 | 1105 | yes |  |
| 4x2 | anonymous public owners | full+spl | yes | 2481 | 29 | 3 | - | 1262 | 1109 | yes |  |
| 4x2 | anonymous public owners | full+data | yes | 2218 | 22 | 3 | - | 1235 | 1206 | yes |  |
| 4x2 | anonymous public owners | full+reply | yes | 2212 | 22 | 3 | - | 1229 | 1202 | yes |  |
| 4x2 | anonymous public owners | full+2tree | yes | 2151 | 23 | 3 | - | 1135 | 1109 | yes |  |
| 4x2 | anonymous public owners | full+data+ring | yes | 2250 | 22 | 3 | - | 1267 | 1238 | yes |  |
| 4x2 | anonymous public owners | full+data+program | yes | 2185 | 19 | 2 | - | 1199 | 1170 | yes |  |
| 4x2 | anonymous public owners | full+data+ring+program | yes | 2217 | 19 | 2 | - | 1231 | 1202 | yes |  |
| 4x2 | confidential entry | min | yes | 2006 | 12 | 1 | - | 1491 | 1508 | yes |  |
| 4x2 | confidential entry | full | yes | 2665 | 22 | 3 | 4 | 1682 | 1657 | yes |  |
| 4x2 | confidential entry | full+spl | yes | 3033 | 29 | 3 | - | 1814 | 1661 | yes |  |
| 4x2 | confidential entry | full+data | yes | 2770 | 22 | 3 | - | 1787 | 1758 | yes |  |
| 4x2 | confidential entry | full+reply | yes | 2665 | 22 | 3 | - | 1682 | 1657 | yes |  |
| 4x2 | confidential entry | full+2tree | yes | 2703 | 23 | 3 | - | 1687 | 1661 | yes |  |
| 4x2 | confidential entry | full+data+ring | yes | 2802 | 22 | 3 | - | 1819 | 1790 | yes |  |
| 4x2 | anonymous entry | min | yes | 2101 | 12 | 1 | - | 1587 | 1605 | yes |  |
| 4x2 | anonymous entry | full | yes | 2568 | 18 | 2 | 4 | 1778 | 1754 | yes |  |
| 4x2 | anonymous entry | full+spl | yes | 2936 | 25 | 2 | - | 1910 | 1758 | yes |  |
| 4x2 | anonymous entry | full+data | yes | 2673 | 18 | 2 | - | 1883 | 1855 | yes |  |
| 4x2 | anonymous entry | full+reply | yes | 2667 | 18 | 2 | - | 1877 | 1851 | yes |  |
| 4x2 | anonymous entry | full+2tree | yes | 2606 | 19 | 2 | - | 1783 | 1758 | yes |  |
| 4x2 | anonymous entry | full+data+ring | yes | 2705 | 18 | 2 | - | 1915 | 1887 | yes |  |
| 4x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x4 | confidential signer | min | yes | 1808 | 12 | 1 | - | 1293 | 1382 | yes |  |
| 4x4 | confidential signer | full | yes | 2467 | 22 | 3 | 4 | 1484 | 1531 | yes |  |
| 4x4 | confidential signer | full+spl | yes | 2835 | 29 | 3 | - | 1616 | 1535 | yes |  |
| 4x4 | confidential signer | full+data | yes | 2644 | 22 | 3 | - | 1661 | 1700 | yes |  |
| 4x4 | confidential signer | full+reply | yes | 2467 | 22 | 3 | - | 1484 | 1531 | yes |  |
| 4x4 | confidential signer | full+2tree | yes | 2505 | 23 | 3 | - | 1489 | 1535 | yes |  |
| 4x4 | confidential signer | full+data+ring | yes | 2740 | 22 | 3 | - | 1757 | 1796 | yes |  |
| 4x4 | confidential signer | full+data+program | yes | 2611 | 19 | 2 | - | 1625 | 1664 | yes |  |
| 4x4 | confidential signer | full+data+ring+program | yes | 2707 | 19 | 2 | - | 1721 | 1760 | yes |  |
| 4x4 | confidential in-proof | min | yes | 1903 | 12 | 1 | - | 1389 | 1479 | yes |  |
| 4x4 | confidential in-proof | full | yes | 2466 | 21 | 2 | 4 | 1580 | 1628 | yes |  |
| 4x4 | confidential in-proof | full+spl | yes | 2834 | 28 | 2 | - | 1712 | 1632 | yes |  |
| 4x4 | confidential in-proof | full+data | yes | 2643 | 21 | 2 | - | 1757 | 1797 | yes |  |
| 4x4 | confidential in-proof | full+reply | yes | 2466 | 21 | 2 | - | 1580 | 1628 | yes |  |
| 4x4 | confidential in-proof | full+2tree | yes | 2504 | 22 | 2 | - | 1585 | 1632 | yes |  |
| 4x4 | confidential in-proof | full+data+ring | yes | 2739 | 21 | 2 | - | 1853 | 1893 | yes |  |
| 4x4 | anonymous hidden | min | yes | 1871 | 12 | 1 | - | 1357 | 1479 | yes |  |
| 4x4 | anonymous hidden | full | yes | 2338 | 18 | 2 | 4 | 1548 | 1628 | yes |  |
| 4x4 | anonymous hidden | full+spl | yes | 2706 | 25 | 2 | - | 1680 | 1632 | yes |  |
| 4x4 | anonymous hidden | full+data | yes | 2515 | 18 | 2 | - | 1725 | 1797 | yes |  |
| 4x4 | anonymous hidden | full+reply | yes | 2631 | 18 | 2 | - | 1841 | 1919 | yes |  |
| 4x4 | anonymous hidden | full+2tree | yes | 2376 | 19 | 2 | - | 1553 | 1632 | yes |  |
| 4x4 | anonymous hidden | full+data+ring | yes | 2611 | 18 | 2 | - | 1821 | 1893 | yes |  |
| 4x4 | anonymous public owners | min | yes | 1808 | 12 | 1 | - | 1293 | 1382 | yes |  |
| 4x4 | anonymous public owners | full | yes | 2467 | 22 | 3 | 4 | 1484 | 1531 | yes |  |
| 4x4 | anonymous public owners | full+spl | yes | 2835 | 29 | 3 | - | 1616 | 1535 | yes |  |
| 4x4 | anonymous public owners | full+data | yes | 2644 | 22 | 3 | - | 1661 | 1700 | yes |  |
| 4x4 | anonymous public owners | full+reply | yes | 2760 | 22 | 3 | - | 1777 | 1822 | yes |  |
| 4x4 | anonymous public owners | full+2tree | yes | 2505 | 23 | 3 | - | 1489 | 1535 | yes |  |
| 4x4 | anonymous public owners | full+data+ring | yes | 2740 | 22 | 3 | - | 1757 | 1796 | yes |  |
| 4x4 | anonymous public owners | full+data+program | yes | 2611 | 19 | 2 | - | 1625 | 1664 | yes |  |
| 4x4 | anonymous public owners | full+data+ring+program | yes | 2707 | 19 | 2 | - | 1721 | 1760 | yes |  |
| 4x4 | confidential entry | min | yes | 2360 | 12 | 1 | - | 1845 | 1934 | yes |  |
| 4x4 | confidential entry | full | yes | 3019 | 22 | 3 | 4 | 2036 | 2083 | yes |  |
| 4x4 | confidential entry | full+spl | yes | 3387 | 29 | 3 | - | 2168 | 2087 | yes |  |
| 4x4 | confidential entry | full+data | yes | 3196 | 22 | 3 | - | 2213 | 2252 | yes |  |
| 4x4 | confidential entry | full+reply | yes | 3019 | 22 | 3 | - | 2036 | 2083 | yes |  |
| 4x4 | confidential entry | full+2tree | yes | 3057 | 23 | 3 | - | 2041 | 2087 | yes |  |
| 4x4 | confidential entry | full+data+ring | yes | 3292 | 22 | 3 | - | 2309 | 2348 | yes |  |
| 4x4 | anonymous entry | min | yes | 2455 | 12 | 1 | - | 1941 | 2031 | yes |  |
| 4x4 | anonymous entry | full | yes | 2922 | 18 | 2 | 4 | 2132 | 2180 | yes |  |
| 4x4 | anonymous entry | full+spl | yes | 3290 | 25 | 2 | - | 2264 | 2184 | yes |  |
| 4x4 | anonymous entry | full+data | yes | 3099 | 18 | 2 | - | 2309 | 2349 | yes |  |
| 4x4 | anonymous entry | full+reply | yes | 3215 | 18 | 2 | - | 2425 | 2471 | yes |  |
| 4x4 | anonymous entry | full+2tree | yes | 2960 | 19 | 2 | - | 2137 | 2184 | yes |  |
| 4x4 | anonymous entry | full+data+ring | yes | 3195 | 18 | 2 | - | 2405 | 2445 | yes |  |
| 4x4 | delegate | min | yes | 1808 | 12 | 1 | - | 1293 | 1382 | yes |  |
| 4x4 | delegate | full | yes | 2255 | 18 | 2 | - | 1468 | 1531 | yes |  |
| 4x4 | delegate | full+spl | yes | 2495 | 25 | 2 | - | 1472 | 1535 | yes |  |
| 4x4 | delegate | full+data | yes | 2432 | 18 | 2 | - | 1645 | 1700 | yes |  |
| 4x4 | delegate | full+reply | yes | 2255 | 18 | 2 | - | 1468 | 1531 | yes |  |
| 4x4 | delegate | full+2tree | yes | 2293 | 19 | 2 | - | 1473 | 1535 | yes |  |
| 4x4 | delegate | full+data+ring | yes | 2528 | 18 | 2 | - | 1741 | 1796 | yes |  |
| 4x4 | anonymous delegate | min | yes | 1808 | 12 | 1 | - | 1293 | 1382 | yes |  |
| 4x4 | anonymous delegate | full | yes | 2255 | 18 | 2 | - | 1468 | 1531 | yes |  |
| 4x4 | anonymous delegate | full+spl | yes | 2495 | 25 | 2 | - | 1472 | 1535 | yes |  |
| 4x4 | anonymous delegate | full+data | yes | 2432 | 18 | 2 | - | 1645 | 1700 | yes |  |
| 4x4 | anonymous delegate | full+reply | yes | 2548 | 18 | 2 | - | 1761 | 1822 | yes |  |
| 4x4 | anonymous delegate | full+2tree | yes | 2293 | 19 | 2 | - | 1473 | 1535 | yes |  |
| 4x4 | anonymous delegate | full+data+ring | yes | 2528 | 18 | 2 | - | 1741 | 1796 | yes |  |
| 4x8 | confidential signer | min | yes | 2516 | 12 | 1 | - | 2001 | 2234 | yes |  |
| 4x8 | confidential signer | full | yes | 3175 | 22 | 3 | 4 | 2192 | 2383 | yes |  |
| 4x8 | confidential signer | full+spl | yes | 3543 | 29 | 3 | - | 2324 | 2387 | yes |  |
| 4x8 | confidential signer | full+data | yes | 3496 | 22 | 3 | - | 2513 | 2688 | yes |  |
| 4x8 | confidential signer | full+reply | yes | 3175 | 22 | 3 | - | 2192 | 2383 | yes |  |
| 4x8 | confidential signer | full+2tree | yes | 3213 | 23 | 3 | - | 2197 | 2387 | yes |  |
| 4x8 | confidential signer | full+data+ring | yes | 3720 | 22 | 3 | - | 2737 | 2912 | yes |  |
| 4x8 | confidential signer | full+data+program | yes | 3463 | 19 | 2 | - | 2477 | 2652 | yes |  |
| 4x8 | confidential signer | full+data+ring+program | yes | 3687 | 19 | 2 | - | 2701 | 2876 | yes |  |
| 4x8 | confidential in-proof | min | yes | 2611 | 12 | 1 | - | 2097 | 2331 | yes |  |
| 4x8 | confidential in-proof | full | yes | 3174 | 21 | 2 | 4 | 2288 | 2480 | yes |  |
| 4x8 | confidential in-proof | full+spl | yes | 3542 | 28 | 2 | - | 2420 | 2484 | yes |  |
| 4x8 | confidential in-proof | full+data | yes | 3495 | 21 | 2 | - | 2609 | 2785 | yes |  |
| 4x8 | confidential in-proof | full+reply | yes | 3174 | 21 | 2 | - | 2288 | 2480 | yes |  |
| 4x8 | confidential in-proof | full+2tree | yes | 3212 | 22 | 2 | - | 2293 | 2484 | yes |  |
| 4x8 | confidential in-proof | full+data+ring | yes | 3719 | 21 | 2 | - | 2833 | 3009 | yes |  |
| 4x8 | anonymous hidden | min | yes | 2579 | 12 | 1 | - | 2065 | 2331 | yes |  |
| 4x8 | anonymous hidden | full | yes | 3046 | 18 | 2 | 4 | 2256 | 2480 | yes |  |
| 4x8 | anonymous hidden | full+spl | yes | 3414 | 25 | 2 | - | 2388 | 2484 | yes |  |
| 4x8 | anonymous hidden | full+data | yes | 3367 | 18 | 2 | - | 2577 | 2785 | yes |  |
| 4x8 | anonymous hidden | full+reply | yes | 3727 | 18 | 2 | - | 2937 | 3159 | yes |  |
| 4x8 | anonymous hidden | full+2tree | yes | 3084 | 19 | 2 | - | 2261 | 2484 | yes |  |
| 4x8 | anonymous hidden | full+data+ring | yes | 3591 | 18 | 2 | - | 2801 | 3009 | yes |  |
| 4x8 | anonymous public owners | min | yes | 2516 | 12 | 1 | - | 2001 | 2234 | yes |  |
| 4x8 | anonymous public owners | full | yes | 3175 | 22 | 3 | 4 | 2192 | 2383 | yes |  |
| 4x8 | anonymous public owners | full+spl | yes | 3543 | 29 | 3 | - | 2324 | 2387 | yes |  |
| 4x8 | anonymous public owners | full+data | yes | 3496 | 22 | 3 | - | 2513 | 2688 | yes |  |
| 4x8 | anonymous public owners | full+reply | yes | 3856 | 22 | 3 | - | 2873 | 3062 | yes |  |
| 4x8 | anonymous public owners | full+2tree | yes | 3213 | 23 | 3 | - | 2197 | 2387 | yes |  |
| 4x8 | anonymous public owners | full+data+ring | yes | 3720 | 22 | 3 | - | 2737 | 2912 | yes |  |
| 4x8 | anonymous public owners | full+data+program | yes | 3463 | 19 | 2 | - | 2477 | 2652 | yes |  |
| 4x8 | anonymous public owners | full+data+ring+program | yes | 3687 | 19 | 2 | - | 2701 | 2876 | yes |  |
| 4x8 | confidential entry | min | yes | 3068 | 12 | 1 | - | 2553 | 2786 | yes |  |
| 4x8 | confidential entry | full | yes | 3727 | 22 | 3 | 4 | 2744 | 2935 | yes |  |
| 4x8 | confidential entry | full+spl | yes | 4095 | 29 | 3 | - | 2876 | 2939 | yes |  |
| 4x8 | confidential entry | full+data | yes | 4048 | 22 | 3 | - | 3065 | 3240 | yes |  |
| 4x8 | confidential entry | full+reply | yes | 3727 | 22 | 3 | - | 2744 | 2935 | yes |  |
| 4x8 | confidential entry | full+2tree | yes | 3765 | 23 | 3 | - | 2749 | 2939 | yes |  |
| 4x8 | confidential entry | full+data+ring | yes | 4272 | 22 | 3 | - | 3289 | 3464 | no | 176 B |
| 4x8 | anonymous entry | min | yes | 3163 | 12 | 1 | - | 2649 | 2883 | yes |  |
| 4x8 | anonymous entry | full | yes | 3630 | 18 | 2 | 4 | 2840 | 3032 | yes |  |
| 4x8 | anonymous entry | full+spl | yes | 3998 | 25 | 2 | - | 2972 | 3036 | yes |  |
| 4x8 | anonymous entry | full+data | yes | 3951 | 18 | 2 | - | 3161 | 3337 | yes |  |
| 4x8 | anonymous entry | full+reply | yes | 4311 | 18 | 2 | - | 3521 | 3711 | no | 215 B |
| 4x8 | anonymous entry | full+2tree | yes | 3668 | 19 | 2 | - | 2845 | 3036 | yes |  |
| 4x8 | anonymous entry | full+data+ring | yes | 4175 | 18 | 2 | - | 3385 | 3561 | no | 79 B |
| 4x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x2 | confidential signer | min | yes | 1519 | 13 | 1 | - | 971 | 989 | yes |  |
| 5x2 | confidential signer | full | yes | 2178 | 23 | 3 | 5 | 1162 | 1138 | yes |  |
| 5x2 | confidential signer | full+spl | yes | 2546 | 30 | 3 | - | 1294 | 1142 | yes |  |
| 5x2 | confidential signer | full+data | yes | 2283 | 23 | 3 | - | 1267 | 1239 | yes |  |
| 5x2 | confidential signer | full+reply | yes | 2178 | 23 | 3 | - | 1162 | 1138 | yes |  |
| 5x2 | confidential signer | full+2tree | yes | 2216 | 24 | 3 | - | 1167 | 1142 | yes |  |
| 5x2 | confidential signer | full+data+ring | yes | 2315 | 23 | 3 | - | 1299 | 1271 | yes |  |
| 5x2 | confidential signer | full+data+program | yes | 2250 | 20 | 2 | - | 1231 | 1203 | yes |  |
| 5x2 | confidential signer | full+data+ring+program | yes | 2282 | 20 | 2 | - | 1263 | 1235 | yes |  |
| 5x2 | confidential in-proof | min | yes | 1614 | 13 | 1 | - | 1067 | 1086 | yes |  |
| 5x2 | confidential in-proof | full | yes | 2177 | 22 | 2 | 5 | 1258 | 1235 | yes |  |
| 5x2 | confidential in-proof | full+spl | yes | 2545 | 29 | 2 | - | 1390 | 1239 | yes |  |
| 5x2 | confidential in-proof | full+data | yes | 2282 | 22 | 2 | - | 1363 | 1336 | yes |  |
| 5x2 | confidential in-proof | full+reply | yes | 2177 | 22 | 2 | - | 1258 | 1235 | yes |  |
| 5x2 | confidential in-proof | full+2tree | yes | 2215 | 23 | 2 | - | 1263 | 1239 | yes |  |
| 5x2 | confidential in-proof | full+data+ring | yes | 2314 | 22 | 2 | - | 1395 | 1368 | yes |  |
| 5x2 | anonymous hidden | min | yes | 1582 | 13 | 1 | - | 1035 | 1086 | yes |  |
| 5x2 | anonymous hidden | full | yes | 2049 | 19 | 2 | 5 | 1226 | 1235 | yes |  |
| 5x2 | anonymous hidden | full+spl | yes | 2417 | 26 | 2 | - | 1358 | 1239 | yes |  |
| 5x2 | anonymous hidden | full+data | yes | 2154 | 19 | 2 | - | 1331 | 1336 | yes |  |
| 5x2 | anonymous hidden | full+reply | yes | 2148 | 19 | 2 | - | 1325 | 1332 | yes |  |
| 5x2 | anonymous hidden | full+2tree | yes | 2087 | 20 | 2 | - | 1231 | 1239 | yes |  |
| 5x2 | anonymous hidden | full+data+ring | yes | 2186 | 19 | 2 | - | 1363 | 1368 | yes |  |
| 5x2 | anonymous public owners | min | yes | 1519 | 13 | 1 | - | 971 | 989 | yes |  |
| 5x2 | anonymous public owners | full | yes | 2178 | 23 | 3 | 5 | 1162 | 1138 | yes |  |
| 5x2 | anonymous public owners | full+spl | yes | 2546 | 30 | 3 | - | 1294 | 1142 | yes |  |
| 5x2 | anonymous public owners | full+data | yes | 2283 | 23 | 3 | - | 1267 | 1239 | yes |  |
| 5x2 | anonymous public owners | full+reply | yes | 2277 | 23 | 3 | - | 1261 | 1235 | yes |  |
| 5x2 | anonymous public owners | full+2tree | yes | 2216 | 24 | 3 | - | 1167 | 1142 | yes |  |
| 5x2 | anonymous public owners | full+data+ring | yes | 2315 | 23 | 3 | - | 1299 | 1271 | yes |  |
| 5x2 | anonymous public owners | full+data+program | yes | 2250 | 20 | 2 | - | 1231 | 1203 | yes |  |
| 5x2 | anonymous public owners | full+data+ring+program | yes | 2282 | 20 | 2 | - | 1263 | 1235 | yes |  |
| 5x2 | confidential entry | min | yes | 2207 | 13 | 1 | - | 1659 | 1677 | yes |  |
| 5x2 | confidential entry | full | yes | 2866 | 23 | 3 | 5 | 1850 | 1826 | yes |  |
| 5x2 | confidential entry | full+spl | yes | 3234 | 30 | 3 | - | 1982 | 1830 | yes |  |
| 5x2 | confidential entry | full+data | yes | 2971 | 23 | 3 | - | 1955 | 1927 | yes |  |
| 5x2 | confidential entry | full+reply | yes | 2866 | 23 | 3 | - | 1850 | 1826 | yes |  |
| 5x2 | confidential entry | full+2tree | yes | 2904 | 24 | 3 | - | 1855 | 1830 | yes |  |
| 5x2 | confidential entry | full+data+ring | yes | 3003 | 23 | 3 | - | 1987 | 1959 | yes |  |
| 5x2 | anonymous entry | min | yes | 2302 | 13 | 1 | - | 1755 | 1774 | yes |  |
| 5x2 | anonymous entry | full | yes | 2769 | 19 | 2 | 5 | 1946 | 1923 | yes |  |
| 5x2 | anonymous entry | full+spl | yes | 3137 | 26 | 2 | - | 2078 | 1927 | yes |  |
| 5x2 | anonymous entry | full+data | yes | 2874 | 19 | 2 | - | 2051 | 2024 | yes |  |
| 5x2 | anonymous entry | full+reply | yes | 2868 | 19 | 2 | - | 2045 | 2020 | yes |  |
| 5x2 | anonymous entry | full+2tree | yes | 2807 | 20 | 2 | - | 1951 | 1927 | yes |  |
| 5x2 | anonymous entry | full+data+ring | yes | 2906 | 19 | 2 | - | 2083 | 2056 | yes |  |
| 5x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x4 | confidential signer | min | yes | 1873 | 13 | 1 | - | 1325 | 1415 | yes |  |
| 5x4 | confidential signer | full | yes | 2532 | 23 | 3 | 5 | 1516 | 1564 | yes |  |
| 5x4 | confidential signer | full+spl | yes | 2900 | 30 | 3 | - | 1648 | 1568 | yes |  |
| 5x4 | confidential signer | full+data | yes | 2709 | 23 | 3 | - | 1693 | 1733 | yes |  |
| 5x4 | confidential signer | full+reply | yes | 2532 | 23 | 3 | - | 1516 | 1564 | yes |  |
| 5x4 | confidential signer | full+2tree | yes | 2570 | 24 | 3 | - | 1521 | 1568 | yes |  |
| 5x4 | confidential signer | full+data+ring | yes | 2805 | 23 | 3 | - | 1789 | 1829 | yes |  |
| 5x4 | confidential signer | full+data+program | yes | 2676 | 20 | 2 | - | 1657 | 1697 | yes |  |
| 5x4 | confidential signer | full+data+ring+program | yes | 2772 | 20 | 2 | - | 1753 | 1793 | yes |  |
| 5x4 | confidential in-proof | min | yes | 1968 | 13 | 1 | - | 1421 | 1512 | yes |  |
| 5x4 | confidential in-proof | full | yes | 2531 | 22 | 2 | 5 | 1612 | 1661 | yes |  |
| 5x4 | confidential in-proof | full+spl | yes | 2899 | 29 | 2 | - | 1744 | 1665 | yes |  |
| 5x4 | confidential in-proof | full+data | yes | 2708 | 22 | 2 | - | 1789 | 1830 | yes |  |
| 5x4 | confidential in-proof | full+reply | yes | 2531 | 22 | 2 | - | 1612 | 1661 | yes |  |
| 5x4 | confidential in-proof | full+2tree | yes | 2569 | 23 | 2 | - | 1617 | 1665 | yes |  |
| 5x4 | confidential in-proof | full+data+ring | yes | 2804 | 22 | 2 | - | 1885 | 1926 | yes |  |
| 5x4 | anonymous hidden | min | yes | 1936 | 13 | 1 | - | 1389 | 1512 | yes |  |
| 5x4 | anonymous hidden | full | yes | 2403 | 19 | 2 | 5 | 1580 | 1661 | yes |  |
| 5x4 | anonymous hidden | full+spl | yes | 2771 | 26 | 2 | - | 1712 | 1665 | yes |  |
| 5x4 | anonymous hidden | full+data | yes | 2580 | 19 | 2 | - | 1757 | 1830 | yes |  |
| 5x4 | anonymous hidden | full+reply | yes | 2696 | 19 | 2 | - | 1873 | 1952 | yes |  |
| 5x4 | anonymous hidden | full+2tree | yes | 2441 | 20 | 2 | - | 1585 | 1665 | yes |  |
| 5x4 | anonymous hidden | full+data+ring | yes | 2676 | 19 | 2 | - | 1853 | 1926 | yes |  |
| 5x4 | anonymous public owners | min | yes | 1873 | 13 | 1 | - | 1325 | 1415 | yes |  |
| 5x4 | anonymous public owners | full | yes | 2532 | 23 | 3 | 5 | 1516 | 1564 | yes |  |
| 5x4 | anonymous public owners | full+spl | yes | 2900 | 30 | 3 | - | 1648 | 1568 | yes |  |
| 5x4 | anonymous public owners | full+data | yes | 2709 | 23 | 3 | - | 1693 | 1733 | yes |  |
| 5x4 | anonymous public owners | full+reply | yes | 2825 | 23 | 3 | - | 1809 | 1855 | yes |  |
| 5x4 | anonymous public owners | full+2tree | yes | 2570 | 24 | 3 | - | 1521 | 1568 | yes |  |
| 5x4 | anonymous public owners | full+data+ring | yes | 2805 | 23 | 3 | - | 1789 | 1829 | yes |  |
| 5x4 | anonymous public owners | full+data+program | yes | 2676 | 20 | 2 | - | 1657 | 1697 | yes |  |
| 5x4 | anonymous public owners | full+data+ring+program | yes | 2772 | 20 | 2 | - | 1753 | 1793 | yes |  |
| 5x4 | confidential entry | min | yes | 2561 | 13 | 1 | - | 2013 | 2103 | yes |  |
| 5x4 | confidential entry | full | yes | 3220 | 23 | 3 | 5 | 2204 | 2252 | yes |  |
| 5x4 | confidential entry | full+spl | yes | 3588 | 30 | 3 | - | 2336 | 2256 | yes |  |
| 5x4 | confidential entry | full+data | yes | 3397 | 23 | 3 | - | 2381 | 2421 | yes |  |
| 5x4 | confidential entry | full+reply | yes | 3220 | 23 | 3 | - | 2204 | 2252 | yes |  |
| 5x4 | confidential entry | full+2tree | yes | 3258 | 24 | 3 | - | 2209 | 2256 | yes |  |
| 5x4 | confidential entry | full+data+ring | yes | 3493 | 23 | 3 | - | 2477 | 2517 | yes |  |
| 5x4 | anonymous entry | min | yes | 2656 | 13 | 1 | - | 2109 | 2200 | yes |  |
| 5x4 | anonymous entry | full | yes | 3123 | 19 | 2 | 5 | 2300 | 2349 | yes |  |
| 5x4 | anonymous entry | full+spl | yes | 3491 | 26 | 2 | - | 2432 | 2353 | yes |  |
| 5x4 | anonymous entry | full+data | yes | 3300 | 19 | 2 | - | 2477 | 2518 | yes |  |
| 5x4 | anonymous entry | full+reply | yes | 3416 | 19 | 2 | - | 2593 | 2640 | yes |  |
| 5x4 | anonymous entry | full+2tree | yes | 3161 | 20 | 2 | - | 2305 | 2353 | yes |  |
| 5x4 | anonymous entry | full+data+ring | yes | 3396 | 19 | 2 | - | 2573 | 2614 | yes |  |
| 5x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x16 | confidential signer | min | yes | 3933 | 12 | 1 | - | 3418 | 3938 | yes |  |
| 4x16 | confidential signer | full | yes | 4592 | 22 | 3 | none | 3609 | 4087 | no | 496 B |
| 4x16 | confidential signer | full+spl | yes | 4960 | 29 | 3 | - | 3741 | 4091 | no | 864 B |
| 4x16 | confidential signer | full+data | yes | 5201 | 22 | 3 | - | 4218 | 4664 | no | 1105 B |
| 4x16 | confidential signer | full+reply | yes | 4592 | 22 | 3 | - | 3609 | 4087 | no | 496 B |
| 4x16 | confidential signer | full+2tree | yes | 4630 | 23 | 3 | - | 3614 | 4091 | no | 534 B |
| 4x16 | confidential signer | full+data+ring | yes | 5681 | 22 | 3 | - | 4698 | 5144 | no | 1585 B |
| 4x16 | confidential signer | full+data+program | yes | 5168 | 19 | 2 | - | 4182 | 4628 | no | 1072 B |
| 4x16 | confidential signer | full+data+ring+program | yes | 5648 | 19 | 2 | - | 4662 | 5108 | no | 1552 B |
| 4x16 | confidential in-proof | min | yes | 4028 | 12 | 1 | - | 3514 | 4035 | yes |  |
| 4x16 | confidential in-proof | full | yes | 4591 | 21 | 2 | none | 3705 | 4184 | no | 495 B |
| 4x16 | confidential in-proof | full+spl | yes | 4959 | 28 | 2 | - | 3837 | 4188 | no | 863 B |
| 4x16 | confidential in-proof | full+data | yes | 5200 | 21 | 2 | - | 4314 | 4761 | no | 1104 B |
| 4x16 | confidential in-proof | full+reply | yes | 4591 | 21 | 2 | - | 3705 | 4184 | no | 495 B |
| 4x16 | confidential in-proof | full+2tree | yes | 4629 | 22 | 2 | - | 3710 | 4188 | no | 533 B |
| 4x16 | confidential in-proof | full+data+ring | yes | 5680 | 21 | 2 | - | 4794 | 5241 | no | 1584 B |
| 4x16 | anonymous hidden | min | yes | 3996 | 12 | 1 | - | 3482 | 4035 | yes |  |
| 4x16 | anonymous hidden | full | yes | 4463 | 18 | 2 | none | 3673 | 4184 | no | 367 B |
| 4x16 | anonymous hidden | full+spl | yes | 4831 | 25 | 2 | - | 3805 | 4188 | no | 735 B |
| 4x16 | anonymous hidden | full+data | yes | 5072 | 18 | 2 | - | 4282 | 4761 | no | 976 B |
| 4x16 | anonymous hidden | full+reply | yes | 5920 | 18 | 2 | - | 5130 | 5639 | no | 1824 B |
| 4x16 | anonymous hidden | full+2tree | yes | 4501 | 19 | 2 | - | 3678 | 4188 | no | 405 B |
| 4x16 | anonymous hidden | full+data+ring | yes | 5552 | 18 | 2 | - | 4762 | 5241 | no | 1456 B |
| 4x16 | anonymous public owners | min | yes | 3933 | 12 | 1 | - | 3418 | 3938 | yes |  |
| 4x16 | anonymous public owners | full | yes | 4592 | 22 | 3 | none | 3609 | 4087 | no | 496 B |
| 4x16 | anonymous public owners | full+spl | yes | 4960 | 29 | 3 | - | 3741 | 4091 | no | 864 B |
| 4x16 | anonymous public owners | full+data | yes | 5201 | 22 | 3 | - | 4218 | 4664 | no | 1105 B |
| 4x16 | anonymous public owners | full+reply | yes | 6049 | 22 | 3 | - | 5066 | 5542 | no | 1953 B |
| 4x16 | anonymous public owners | full+2tree | yes | 4630 | 23 | 3 | - | 3614 | 4091 | no | 534 B |
| 4x16 | anonymous public owners | full+data+ring | yes | 5681 | 22 | 3 | - | 4698 | 5144 | no | 1585 B |
| 4x16 | anonymous public owners | full+data+program | yes | 5168 | 19 | 2 | - | 4182 | 4628 | no | 1072 B |
| 4x16 | anonymous public owners | full+data+ring+program | yes | 5648 | 19 | 2 | - | 4662 | 5108 | no | 1552 B |
| 4x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 4x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 4x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x8 | confidential signer | min | yes | 2581 | 13 | 1 | - | 2033 | 2267 | yes |  |
| 5x8 | confidential signer | full | yes | 3240 | 23 | 3 | 5 | 2224 | 2416 | yes |  |
| 5x8 | confidential signer | full+spl | yes | 3608 | 30 | 3 | - | 2356 | 2420 | yes |  |
| 5x8 | confidential signer | full+data | yes | 3561 | 23 | 3 | - | 2545 | 2721 | yes |  |
| 5x8 | confidential signer | full+reply | yes | 3240 | 23 | 3 | - | 2224 | 2416 | yes |  |
| 5x8 | confidential signer | full+2tree | yes | 3278 | 24 | 3 | - | 2229 | 2420 | yes |  |
| 5x8 | confidential signer | full+data+ring | yes | 3785 | 23 | 3 | - | 2769 | 2945 | yes |  |
| 5x8 | confidential signer | full+data+program | yes | 3528 | 20 | 2 | - | 2509 | 2685 | yes |  |
| 5x8 | confidential signer | full+data+ring+program | yes | 3752 | 20 | 2 | - | 2733 | 2909 | yes |  |
| 5x8 | confidential in-proof | min | yes | 2676 | 13 | 1 | - | 2129 | 2364 | yes |  |
| 5x8 | confidential in-proof | full | yes | 3239 | 22 | 2 | 5 | 2320 | 2513 | yes |  |
| 5x8 | confidential in-proof | full+spl | yes | 3607 | 29 | 2 | - | 2452 | 2517 | yes |  |
| 5x8 | confidential in-proof | full+data | yes | 3560 | 22 | 2 | - | 2641 | 2818 | yes |  |
| 5x8 | confidential in-proof | full+reply | yes | 3239 | 22 | 2 | - | 2320 | 2513 | yes |  |
| 5x8 | confidential in-proof | full+2tree | yes | 3277 | 23 | 2 | - | 2325 | 2517 | yes |  |
| 5x8 | confidential in-proof | full+data+ring | yes | 3784 | 22 | 2 | - | 2865 | 3042 | yes |  |
| 5x8 | anonymous hidden | min | yes | 2644 | 13 | 1 | - | 2097 | 2364 | yes |  |
| 5x8 | anonymous hidden | full | yes | 3111 | 19 | 2 | 5 | 2288 | 2513 | yes |  |
| 5x8 | anonymous hidden | full+spl | yes | 3479 | 26 | 2 | - | 2420 | 2517 | yes |  |
| 5x8 | anonymous hidden | full+data | yes | 3432 | 19 | 2 | - | 2609 | 2818 | yes |  |
| 5x8 | anonymous hidden | full+reply | yes | 3792 | 19 | 2 | - | 2969 | 3192 | yes |  |
| 5x8 | anonymous hidden | full+2tree | yes | 3149 | 20 | 2 | - | 2293 | 2517 | yes |  |
| 5x8 | anonymous hidden | full+data+ring | yes | 3656 | 19 | 2 | - | 2833 | 3042 | yes |  |
| 5x8 | anonymous public owners | min | yes | 2581 | 13 | 1 | - | 2033 | 2267 | yes |  |
| 5x8 | anonymous public owners | full | yes | 3240 | 23 | 3 | 5 | 2224 | 2416 | yes |  |
| 5x8 | anonymous public owners | full+spl | yes | 3608 | 30 | 3 | - | 2356 | 2420 | yes |  |
| 5x8 | anonymous public owners | full+data | yes | 3561 | 23 | 3 | - | 2545 | 2721 | yes |  |
| 5x8 | anonymous public owners | full+reply | yes | 3921 | 23 | 3 | - | 2905 | 3095 | yes |  |
| 5x8 | anonymous public owners | full+2tree | yes | 3278 | 24 | 3 | - | 2229 | 2420 | yes |  |
| 5x8 | anonymous public owners | full+data+ring | yes | 3785 | 23 | 3 | - | 2769 | 2945 | yes |  |
| 5x8 | anonymous public owners | full+data+program | yes | 3528 | 20 | 2 | - | 2509 | 2685 | yes |  |
| 5x8 | anonymous public owners | full+data+ring+program | yes | 3752 | 20 | 2 | - | 2733 | 2909 | yes |  |
| 5x8 | confidential entry | min | yes | 3269 | 13 | 1 | - | 2721 | 2955 | yes |  |
| 5x8 | confidential entry | full | yes | 3928 | 23 | 3 | 2 | 2912 | 3104 | yes |  |
| 5x8 | confidential entry | full+spl | yes | 4296 | 30 | 3 | - | 3044 | 3108 | no | 200 B |
| 5x8 | confidential entry | full+data | yes | 4249 | 23 | 3 | - | 3233 | 3409 | no | 153 B |
| 5x8 | confidential entry | full+reply | yes | 3928 | 23 | 3 | - | 2912 | 3104 | yes |  |
| 5x8 | confidential entry | full+2tree | yes | 3966 | 24 | 3 | - | 2917 | 3108 | yes |  |
| 5x8 | confidential entry | full+data+ring | yes | 4473 | 23 | 3 | - | 3457 | 3633 | no | 377 B |
| 5x8 | anonymous entry | min | yes | 3364 | 13 | 1 | - | 2817 | 3052 | yes |  |
| 5x8 | anonymous entry | full | yes | 3831 | 19 | 2 | 2 | 3008 | 3201 | yes |  |
| 5x8 | anonymous entry | full+spl | yes | 4199 | 26 | 2 | - | 3140 | 3205 | no | 103 B |
| 5x8 | anonymous entry | full+data | yes | 4152 | 19 | 2 | - | 3329 | 3506 | no | 56 B |
| 5x8 | anonymous entry | full+reply | yes | 4512 | 19 | 2 | - | 3689 | 3880 | no | 416 B |
| 5x8 | anonymous entry | full+2tree | yes | 3869 | 20 | 2 | - | 3013 | 3205 | yes |  |
| 5x8 | anonymous entry | full+data+ring | yes | 4376 | 19 | 2 | - | 3553 | 3730 | no | 280 B |
| 5x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x2 | confidential signer | min | yes | 1584 | 14 | 1 | - | 1003 | 1022 | yes |  |
| 6x2 | confidential signer | full | yes | 2243 | 24 | 3 | 6 | 1194 | 1171 | yes |  |
| 6x2 | confidential signer | full+spl | yes | 2611 | 31 | 3 | - | 1326 | 1175 | yes |  |
| 6x2 | confidential signer | full+data | yes | 2348 | 24 | 3 | - | 1299 | 1272 | yes |  |
| 6x2 | confidential signer | full+reply | yes | 2243 | 24 | 3 | - | 1194 | 1171 | yes |  |
| 6x2 | confidential signer | full+2tree | yes | 2281 | 25 | 3 | - | 1199 | 1175 | yes |  |
| 6x2 | confidential signer | full+data+ring | yes | 2380 | 24 | 3 | - | 1331 | 1304 | yes |  |
| 6x2 | confidential signer | full+data+program | yes | 2315 | 21 | 2 | - | 1263 | 1236 | yes |  |
| 6x2 | confidential signer | full+data+ring+program | yes | 2347 | 21 | 2 | - | 1295 | 1268 | yes |  |
| 6x2 | confidential in-proof | min | yes | 1679 | 14 | 1 | - | 1099 | 1119 | yes |  |
| 6x2 | confidential in-proof | full | yes | 2242 | 23 | 2 | 6 | 1290 | 1268 | yes |  |
| 6x2 | confidential in-proof | full+spl | yes | 2610 | 30 | 2 | - | 1422 | 1272 | yes |  |
| 6x2 | confidential in-proof | full+data | yes | 2347 | 23 | 2 | - | 1395 | 1369 | yes |  |
| 6x2 | confidential in-proof | full+reply | yes | 2242 | 23 | 2 | - | 1290 | 1268 | yes |  |
| 6x2 | confidential in-proof | full+2tree | yes | 2280 | 24 | 2 | - | 1295 | 1272 | yes |  |
| 6x2 | confidential in-proof | full+data+ring | yes | 2379 | 23 | 2 | - | 1427 | 1401 | yes |  |
| 6x2 | anonymous hidden | min | yes | 1647 | 14 | 1 | - | 1067 | 1119 | yes |  |
| 6x2 | anonymous hidden | full | yes | 2114 | 20 | 2 | 6 | 1258 | 1268 | yes |  |
| 6x2 | anonymous hidden | full+spl | yes | 2482 | 27 | 2 | - | 1390 | 1272 | yes |  |
| 6x2 | anonymous hidden | full+data | yes | 2219 | 20 | 2 | - | 1363 | 1369 | yes |  |
| 6x2 | anonymous hidden | full+reply | yes | 2213 | 20 | 2 | - | 1357 | 1365 | yes |  |
| 6x2 | anonymous hidden | full+2tree | yes | 2152 | 21 | 2 | - | 1263 | 1272 | yes |  |
| 6x2 | anonymous hidden | full+data+ring | yes | 2251 | 20 | 2 | - | 1395 | 1401 | yes |  |
| 6x2 | anonymous public owners | min | yes | 1584 | 14 | 1 | - | 1003 | 1022 | yes |  |
| 6x2 | anonymous public owners | full | yes | 2243 | 24 | 3 | 6 | 1194 | 1171 | yes |  |
| 6x2 | anonymous public owners | full+spl | yes | 2611 | 31 | 3 | - | 1326 | 1175 | yes |  |
| 6x2 | anonymous public owners | full+data | yes | 2348 | 24 | 3 | - | 1299 | 1272 | yes |  |
| 6x2 | anonymous public owners | full+reply | yes | 2342 | 24 | 3 | - | 1293 | 1268 | yes |  |
| 6x2 | anonymous public owners | full+2tree | yes | 2281 | 25 | 3 | - | 1199 | 1175 | yes |  |
| 6x2 | anonymous public owners | full+data+ring | yes | 2380 | 24 | 3 | - | 1331 | 1304 | yes |  |
| 6x2 | anonymous public owners | full+data+program | yes | 2315 | 21 | 2 | - | 1263 | 1236 | yes |  |
| 6x2 | anonymous public owners | full+data+ring+program | yes | 2347 | 21 | 2 | - | 1295 | 1268 | yes |  |
| 6x2 | confidential entry | min | yes | 2408 | 14 | 1 | - | 1827 | 1846 | yes |  |
| 6x2 | confidential entry | full | yes | 3067 | 24 | 3 | 6 | 2018 | 1995 | yes |  |
| 6x2 | confidential entry | full+spl | yes | 3435 | 31 | 3 | - | 2150 | 1999 | yes |  |
| 6x2 | confidential entry | full+data | yes | 3172 | 24 | 3 | - | 2123 | 2096 | yes |  |
| 6x2 | confidential entry | full+reply | yes | 3067 | 24 | 3 | - | 2018 | 1995 | yes |  |
| 6x2 | confidential entry | full+2tree | yes | 3105 | 25 | 3 | - | 2023 | 1999 | yes |  |
| 6x2 | confidential entry | full+data+ring | yes | 3204 | 24 | 3 | - | 2155 | 2128 | yes |  |
| 6x2 | anonymous entry | min | yes | 2503 | 14 | 1 | - | 1923 | 1943 | yes |  |
| 6x2 | anonymous entry | full | yes | 2970 | 20 | 2 | 6 | 2114 | 2092 | yes |  |
| 6x2 | anonymous entry | full+spl | yes | 3338 | 27 | 2 | - | 2246 | 2096 | yes |  |
| 6x2 | anonymous entry | full+data | yes | 3075 | 20 | 2 | - | 2219 | 2193 | yes |  |
| 6x2 | anonymous entry | full+reply | yes | 3069 | 20 | 2 | - | 2213 | 2189 | yes |  |
| 6x2 | anonymous entry | full+2tree | yes | 3008 | 21 | 2 | - | 2119 | 2096 | yes |  |
| 6x2 | anonymous entry | full+data+ring | yes | 3107 | 20 | 2 | - | 2251 | 2225 | yes |  |
| 6x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x4 | confidential signer | min | yes | 1938 | 14 | 1 | - | 1357 | 1448 | yes |  |
| 6x4 | confidential signer | full | yes | 2597 | 24 | 3 | 6 | 1548 | 1597 | yes |  |
| 6x4 | confidential signer | full+spl | yes | 2965 | 31 | 3 | - | 1680 | 1601 | yes |  |
| 6x4 | confidential signer | full+data | yes | 2774 | 24 | 3 | - | 1725 | 1766 | yes |  |
| 6x4 | confidential signer | full+reply | yes | 2597 | 24 | 3 | - | 1548 | 1597 | yes |  |
| 6x4 | confidential signer | full+2tree | yes | 2635 | 25 | 3 | - | 1553 | 1601 | yes |  |
| 6x4 | confidential signer | full+data+ring | yes | 2870 | 24 | 3 | - | 1821 | 1862 | yes |  |
| 6x4 | confidential signer | full+data+program | yes | 2741 | 21 | 2 | - | 1689 | 1730 | yes |  |
| 6x4 | confidential signer | full+data+ring+program | yes | 2837 | 21 | 2 | - | 1785 | 1826 | yes |  |
| 6x4 | confidential in-proof | min | yes | 2033 | 14 | 1 | - | 1453 | 1545 | yes |  |
| 6x4 | confidential in-proof | full | yes | 2596 | 23 | 2 | 6 | 1644 | 1694 | yes |  |
| 6x4 | confidential in-proof | full+spl | yes | 2964 | 30 | 2 | - | 1776 | 1698 | yes |  |
| 6x4 | confidential in-proof | full+data | yes | 2773 | 23 | 2 | - | 1821 | 1863 | yes |  |
| 6x4 | confidential in-proof | full+reply | yes | 2596 | 23 | 2 | - | 1644 | 1694 | yes |  |
| 6x4 | confidential in-proof | full+2tree | yes | 2634 | 24 | 2 | - | 1649 | 1698 | yes |  |
| 6x4 | confidential in-proof | full+data+ring | yes | 2869 | 23 | 2 | - | 1917 | 1959 | yes |  |
| 6x4 | anonymous hidden | min | yes | 2001 | 14 | 1 | - | 1421 | 1545 | yes |  |
| 6x4 | anonymous hidden | full | yes | 2468 | 20 | 2 | 6 | 1612 | 1694 | yes |  |
| 6x4 | anonymous hidden | full+spl | yes | 2836 | 27 | 2 | - | 1744 | 1698 | yes |  |
| 6x4 | anonymous hidden | full+data | yes | 2645 | 20 | 2 | - | 1789 | 1863 | yes |  |
| 6x4 | anonymous hidden | full+reply | yes | 2761 | 20 | 2 | - | 1905 | 1985 | yes |  |
| 6x4 | anonymous hidden | full+2tree | yes | 2506 | 21 | 2 | - | 1617 | 1698 | yes |  |
| 6x4 | anonymous hidden | full+data+ring | yes | 2741 | 20 | 2 | - | 1885 | 1959 | yes |  |
| 6x4 | anonymous public owners | min | yes | 1938 | 14 | 1 | - | 1357 | 1448 | yes |  |
| 6x4 | anonymous public owners | full | yes | 2597 | 24 | 3 | 6 | 1548 | 1597 | yes |  |
| 6x4 | anonymous public owners | full+spl | yes | 2965 | 31 | 3 | - | 1680 | 1601 | yes |  |
| 6x4 | anonymous public owners | full+data | yes | 2774 | 24 | 3 | - | 1725 | 1766 | yes |  |
| 6x4 | anonymous public owners | full+reply | yes | 2890 | 24 | 3 | - | 1841 | 1888 | yes |  |
| 6x4 | anonymous public owners | full+2tree | yes | 2635 | 25 | 3 | - | 1553 | 1601 | yes |  |
| 6x4 | anonymous public owners | full+data+ring | yes | 2870 | 24 | 3 | - | 1821 | 1862 | yes |  |
| 6x4 | anonymous public owners | full+data+program | yes | 2741 | 21 | 2 | - | 1689 | 1730 | yes |  |
| 6x4 | anonymous public owners | full+data+ring+program | yes | 2837 | 21 | 2 | - | 1785 | 1826 | yes |  |
| 6x4 | confidential entry | min | yes | 2762 | 14 | 1 | - | 2181 | 2272 | yes |  |
| 6x4 | confidential entry | full | yes | 3421 | 24 | 3 | 6 | 2372 | 2421 | yes |  |
| 6x4 | confidential entry | full+spl | yes | 3789 | 31 | 3 | - | 2504 | 2425 | yes |  |
| 6x4 | confidential entry | full+data | yes | 3598 | 24 | 3 | - | 2549 | 2590 | yes |  |
| 6x4 | confidential entry | full+reply | yes | 3421 | 24 | 3 | - | 2372 | 2421 | yes |  |
| 6x4 | confidential entry | full+2tree | yes | 3459 | 25 | 3 | - | 2377 | 2425 | yes |  |
| 6x4 | confidential entry | full+data+ring | yes | 3694 | 24 | 3 | - | 2645 | 2686 | yes |  |
| 6x4 | anonymous entry | min | yes | 2857 | 14 | 1 | - | 2277 | 2369 | yes |  |
| 6x4 | anonymous entry | full | yes | 3324 | 20 | 2 | 6 | 2468 | 2518 | yes |  |
| 6x4 | anonymous entry | full+spl | yes | 3692 | 27 | 2 | - | 2600 | 2522 | yes |  |
| 6x4 | anonymous entry | full+data | yes | 3501 | 20 | 2 | - | 2645 | 2687 | yes |  |
| 6x4 | anonymous entry | full+reply | yes | 3617 | 20 | 2 | - | 2761 | 2809 | yes |  |
| 6x4 | anonymous entry | full+2tree | yes | 3362 | 21 | 2 | - | 2473 | 2522 | yes |  |
| 6x4 | anonymous entry | full+data+ring | yes | 3597 | 20 | 2 | - | 2741 | 2783 | yes |  |
| 6x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x16 | confidential signer | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+data+program | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential signer | full+data+ring+program | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential in-proof | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous hidden | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+data+program | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous public owners | full+data+ring+program | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 5x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 5x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x8 | confidential signer | min | yes | 2646 | 14 | 1 | - | 2065 | 2300 | yes |  |
| 6x8 | confidential signer | full | yes | 3305 | 24 | 3 | 6 | 2256 | 2449 | yes |  |
| 6x8 | confidential signer | full+spl | yes | 3673 | 31 | 3 | - | 2388 | 2453 | yes |  |
| 6x8 | confidential signer | full+data | yes | 3626 | 24 | 3 | - | 2577 | 2754 | yes |  |
| 6x8 | confidential signer | full+reply | yes | 3305 | 24 | 3 | - | 2256 | 2449 | yes |  |
| 6x8 | confidential signer | full+2tree | yes | 3343 | 25 | 3 | - | 2261 | 2453 | yes |  |
| 6x8 | confidential signer | full+data+ring | yes | 3850 | 24 | 3 | - | 2801 | 2978 | yes |  |
| 6x8 | confidential signer | full+data+program | yes | 3593 | 21 | 2 | - | 2541 | 2718 | yes |  |
| 6x8 | confidential signer | full+data+ring+program | yes | 3817 | 21 | 2 | - | 2765 | 2942 | yes |  |
| 6x8 | confidential in-proof | min | yes | 2741 | 14 | 1 | - | 2161 | 2397 | yes |  |
| 6x8 | confidential in-proof | full | yes | 3304 | 23 | 2 | 6 | 2352 | 2546 | yes |  |
| 6x8 | confidential in-proof | full+spl | yes | 3672 | 30 | 2 | - | 2484 | 2550 | yes |  |
| 6x8 | confidential in-proof | full+data | yes | 3625 | 23 | 2 | - | 2673 | 2851 | yes |  |
| 6x8 | confidential in-proof | full+reply | yes | 3304 | 23 | 2 | - | 2352 | 2546 | yes |  |
| 6x8 | confidential in-proof | full+2tree | yes | 3342 | 24 | 2 | - | 2357 | 2550 | yes |  |
| 6x8 | confidential in-proof | full+data+ring | yes | 3849 | 23 | 2 | - | 2897 | 3075 | yes |  |
| 6x8 | anonymous hidden | min | yes | 2709 | 14 | 1 | - | 2129 | 2397 | yes |  |
| 6x8 | anonymous hidden | full | yes | 3176 | 20 | 2 | 6 | 2320 | 2546 | yes |  |
| 6x8 | anonymous hidden | full+spl | yes | 3544 | 27 | 2 | - | 2452 | 2550 | yes |  |
| 6x8 | anonymous hidden | full+data | yes | 3497 | 20 | 2 | - | 2641 | 2851 | yes |  |
| 6x8 | anonymous hidden | full+reply | yes | 3857 | 20 | 2 | - | 3001 | 3225 | yes |  |
| 6x8 | anonymous hidden | full+2tree | yes | 3214 | 21 | 2 | - | 2325 | 2550 | yes |  |
| 6x8 | anonymous hidden | full+data+ring | yes | 3721 | 20 | 2 | - | 2865 | 3075 | yes |  |
| 6x8 | anonymous public owners | min | yes | 2646 | 14 | 1 | - | 2065 | 2300 | yes |  |
| 6x8 | anonymous public owners | full | yes | 3305 | 24 | 3 | 6 | 2256 | 2449 | yes |  |
| 6x8 | anonymous public owners | full+spl | yes | 3673 | 31 | 3 | - | 2388 | 2453 | yes |  |
| 6x8 | anonymous public owners | full+data | yes | 3626 | 24 | 3 | - | 2577 | 2754 | yes |  |
| 6x8 | anonymous public owners | full+reply | yes | 3986 | 24 | 3 | - | 2937 | 3128 | yes |  |
| 6x8 | anonymous public owners | full+2tree | yes | 3343 | 25 | 3 | - | 2261 | 2453 | yes |  |
| 6x8 | anonymous public owners | full+data+ring | yes | 3850 | 24 | 3 | - | 2801 | 2978 | yes |  |
| 6x8 | anonymous public owners | full+data+program | yes | 3593 | 21 | 2 | - | 2541 | 2718 | yes |  |
| 6x8 | anonymous public owners | full+data+ring+program | yes | 3817 | 21 | 2 | - | 2765 | 2942 | yes |  |
| 6x8 | confidential entry | min | yes | 3470 | 14 | 1 | - | 2889 | 3124 | yes |  |
| 6x8 | confidential entry | full | yes | 4129 | 24 | 3 | none | 3080 | 3273 | no | 33 B |
| 6x8 | confidential entry | full+spl | yes | 4497 | 31 | 3 | - | 3212 | 3277 | no | 401 B |
| 6x8 | confidential entry | full+data | yes | 4450 | 24 | 3 | - | 3401 | 3578 | no | 354 B |
| 6x8 | confidential entry | full+reply | yes | 4129 | 24 | 3 | - | 3080 | 3273 | no | 33 B |
| 6x8 | confidential entry | full+2tree | yes | 4167 | 25 | 3 | - | 3085 | 3277 | no | 71 B |
| 6x8 | confidential entry | full+data+ring | yes | 4674 | 24 | 3 | - | 3625 | 3802 | no | 578 B |
| 6x8 | anonymous entry | min | yes | 3565 | 14 | 1 | - | 2985 | 3221 | yes |  |
| 6x8 | anonymous entry | full | yes | 4032 | 20 | 2 | 0 | 3176 | 3370 | yes |  |
| 6x8 | anonymous entry | full+spl | yes | 4400 | 27 | 2 | - | 3308 | 3374 | no | 304 B |
| 6x8 | anonymous entry | full+data | yes | 4353 | 20 | 2 | - | 3497 | 3675 | no | 257 B |
| 6x8 | anonymous entry | full+reply | yes | 4713 | 20 | 2 | - | 3857 | 4049 | no | 617 B |
| 6x8 | anonymous entry | full+2tree | yes | 4070 | 21 | 2 | - | 3181 | 3374 | yes |  |
| 6x8 | anonymous entry | full+data+ring | yes | 4577 | 20 | 2 | - | 3721 | 3899 | no | 481 B |
| 6x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 6x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x2 | confidential signer | min | yes | 1714 | 16 | 1 | - | 1067 | 1088 | yes |  |
| 8x2 | confidential signer | full | yes | 2373 | 26 | 3 | 8 | 1258 | 1237 | yes |  |
| 8x2 | confidential signer | full+spl | yes | 2741 | 33 | 3 | - | 1390 | 1241 | yes |  |
| 8x2 | confidential signer | full+data | yes | 2478 | 26 | 3 | - | 1363 | 1338 | yes |  |
| 8x2 | confidential signer | full+reply | yes | 2373 | 26 | 3 | - | 1258 | 1237 | yes |  |
| 8x2 | confidential signer | full+2tree | yes | 2411 | 27 | 3 | - | 1263 | 1241 | yes |  |
| 8x2 | confidential signer | full+data+ring | yes | 2510 | 26 | 3 | - | 1395 | 1370 | yes |  |
| 8x2 | confidential signer | full+data+program | yes | 2445 | 23 | 2 | - | 1327 | 1302 | yes |  |
| 8x2 | confidential signer | full+data+ring+program | yes | 2477 | 23 | 2 | - | 1359 | 1334 | yes |  |
| 8x2 | confidential in-proof | min | yes | 1809 | 16 | 1 | - | 1163 | 1185 | yes |  |
| 8x2 | confidential in-proof | full | yes | 2372 | 25 | 2 | 8 | 1354 | 1334 | yes |  |
| 8x2 | confidential in-proof | full+spl | yes | 2740 | 32 | 2 | - | 1486 | 1338 | yes |  |
| 8x2 | confidential in-proof | full+data | yes | 2477 | 25 | 2 | - | 1459 | 1435 | yes |  |
| 8x2 | confidential in-proof | full+reply | yes | 2372 | 25 | 2 | - | 1354 | 1334 | yes |  |
| 8x2 | confidential in-proof | full+2tree | yes | 2410 | 26 | 2 | - | 1359 | 1338 | yes |  |
| 8x2 | confidential in-proof | full+data+ring | yes | 2509 | 25 | 2 | - | 1491 | 1467 | yes |  |
| 8x2 | anonymous hidden | min | yes | 1777 | 16 | 1 | - | 1131 | 1185 | yes |  |
| 8x2 | anonymous hidden | full | yes | 2244 | 22 | 2 | 8 | 1322 | 1334 | yes |  |
| 8x2 | anonymous hidden | full+spl | yes | 2612 | 29 | 2 | - | 1454 | 1338 | yes |  |
| 8x2 | anonymous hidden | full+data | yes | 2349 | 22 | 2 | - | 1427 | 1435 | yes |  |
| 8x2 | anonymous hidden | full+reply | yes | 2343 | 22 | 2 | - | 1421 | 1431 | yes |  |
| 8x2 | anonymous hidden | full+2tree | yes | 2282 | 23 | 2 | - | 1327 | 1338 | yes |  |
| 8x2 | anonymous hidden | full+data+ring | yes | 2381 | 22 | 2 | - | 1459 | 1467 | yes |  |
| 8x2 | anonymous public owners | min | yes | 1714 | 16 | 1 | - | 1067 | 1088 | yes |  |
| 8x2 | anonymous public owners | full | yes | 2373 | 26 | 3 | 8 | 1258 | 1237 | yes |  |
| 8x2 | anonymous public owners | full+spl | yes | 2741 | 33 | 3 | - | 1390 | 1241 | yes |  |
| 8x2 | anonymous public owners | full+data | yes | 2478 | 26 | 3 | - | 1363 | 1338 | yes |  |
| 8x2 | anonymous public owners | full+reply | yes | 2472 | 26 | 3 | - | 1357 | 1334 | yes |  |
| 8x2 | anonymous public owners | full+2tree | yes | 2411 | 27 | 3 | - | 1263 | 1241 | yes |  |
| 8x2 | anonymous public owners | full+data+ring | yes | 2510 | 26 | 3 | - | 1395 | 1370 | yes |  |
| 8x2 | anonymous public owners | full+data+program | yes | 2445 | 23 | 2 | - | 1327 | 1302 | yes |  |
| 8x2 | anonymous public owners | full+data+ring+program | yes | 2477 | 23 | 2 | - | 1359 | 1334 | yes |  |
| 8x2 | confidential entry | min | yes | 2810 | 16 | 1 | - | 2163 | 2184 | yes |  |
| 8x2 | confidential entry | full | yes | 3469 | 26 | 3 | 7 | 2354 | 2333 | yes |  |
| 8x2 | confidential entry | full+spl | yes | 3837 | 33 | 3 | - | 2486 | 2337 | yes |  |
| 8x2 | confidential entry | full+data | yes | 3574 | 26 | 3 | - | 2459 | 2434 | yes |  |
| 8x2 | confidential entry | full+reply | yes | 3469 | 26 | 3 | - | 2354 | 2333 | yes |  |
| 8x2 | confidential entry | full+2tree | yes | 3507 | 27 | 3 | - | 2359 | 2337 | yes |  |
| 8x2 | confidential entry | full+data+ring | yes | 3606 | 26 | 3 | - | 2491 | 2466 | yes |  |
| 8x2 | anonymous entry | min | yes | 2905 | 16 | 1 | - | 2259 | 2281 | yes |  |
| 8x2 | anonymous entry | full | yes | 3372 | 22 | 2 | 7 | 2450 | 2430 | yes |  |
| 8x2 | anonymous entry | full+spl | yes | 3740 | 29 | 2 | - | 2582 | 2434 | yes |  |
| 8x2 | anonymous entry | full+data | yes | 3477 | 22 | 2 | - | 2555 | 2531 | yes |  |
| 8x2 | anonymous entry | full+reply | yes | 3471 | 22 | 2 | - | 2549 | 2527 | yes |  |
| 8x2 | anonymous entry | full+2tree | yes | 3410 | 23 | 2 | - | 2455 | 2434 | yes |  |
| 8x2 | anonymous entry | full+data+ring | yes | 3509 | 22 | 2 | - | 2587 | 2563 | yes |  |
| 8x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x4 | confidential signer | min | yes | 2068 | 16 | 1 | - | 1421 | 1514 | yes |  |
| 8x4 | confidential signer | full | yes | 2727 | 26 | 3 | 8 | 1612 | 1663 | yes |  |
| 8x4 | confidential signer | full+spl | yes | 3095 | 33 | 3 | - | 1744 | 1667 | yes |  |
| 8x4 | confidential signer | full+data | yes | 2904 | 26 | 3 | - | 1789 | 1832 | yes |  |
| 8x4 | confidential signer | full+reply | yes | 2727 | 26 | 3 | - | 1612 | 1663 | yes |  |
| 8x4 | confidential signer | full+2tree | yes | 2765 | 27 | 3 | - | 1617 | 1667 | yes |  |
| 8x4 | confidential signer | full+data+ring | yes | 3000 | 26 | 3 | - | 1885 | 1928 | yes |  |
| 8x4 | confidential signer | full+data+program | yes | 2871 | 23 | 2 | - | 1753 | 1796 | yes |  |
| 8x4 | confidential signer | full+data+ring+program | yes | 2967 | 23 | 2 | - | 1849 | 1892 | yes |  |
| 8x4 | confidential in-proof | min | yes | 2163 | 16 | 1 | - | 1517 | 1611 | yes |  |
| 8x4 | confidential in-proof | full | yes | 2726 | 25 | 2 | 8 | 1708 | 1760 | yes |  |
| 8x4 | confidential in-proof | full+spl | yes | 3094 | 32 | 2 | - | 1840 | 1764 | yes |  |
| 8x4 | confidential in-proof | full+data | yes | 2903 | 25 | 2 | - | 1885 | 1929 | yes |  |
| 8x4 | confidential in-proof | full+reply | yes | 2726 | 25 | 2 | - | 1708 | 1760 | yes |  |
| 8x4 | confidential in-proof | full+2tree | yes | 2764 | 26 | 2 | - | 1713 | 1764 | yes |  |
| 8x4 | confidential in-proof | full+data+ring | yes | 2999 | 25 | 2 | - | 1981 | 2025 | yes |  |
| 8x4 | anonymous hidden | min | yes | 2131 | 16 | 1 | - | 1485 | 1611 | yes |  |
| 8x4 | anonymous hidden | full | yes | 2598 | 22 | 2 | 8 | 1676 | 1760 | yes |  |
| 8x4 | anonymous hidden | full+spl | yes | 2966 | 29 | 2 | - | 1808 | 1764 | yes |  |
| 8x4 | anonymous hidden | full+data | yes | 2775 | 22 | 2 | - | 1853 | 1929 | yes |  |
| 8x4 | anonymous hidden | full+reply | yes | 2891 | 22 | 2 | - | 1969 | 2051 | yes |  |
| 8x4 | anonymous hidden | full+2tree | yes | 2636 | 23 | 2 | - | 1681 | 1764 | yes |  |
| 8x4 | anonymous hidden | full+data+ring | yes | 2871 | 22 | 2 | - | 1949 | 2025 | yes |  |
| 8x4 | anonymous public owners | min | yes | 2068 | 16 | 1 | - | 1421 | 1514 | yes |  |
| 8x4 | anonymous public owners | full | yes | 2727 | 26 | 3 | 8 | 1612 | 1663 | yes |  |
| 8x4 | anonymous public owners | full+spl | yes | 3095 | 33 | 3 | - | 1744 | 1667 | yes |  |
| 8x4 | anonymous public owners | full+data | yes | 2904 | 26 | 3 | - | 1789 | 1832 | yes |  |
| 8x4 | anonymous public owners | full+reply | yes | 3020 | 26 | 3 | - | 1905 | 1954 | yes |  |
| 8x4 | anonymous public owners | full+2tree | yes | 2765 | 27 | 3 | - | 1617 | 1667 | yes |  |
| 8x4 | anonymous public owners | full+data+ring | yes | 3000 | 26 | 3 | - | 1885 | 1928 | yes |  |
| 8x4 | anonymous public owners | full+data+program | yes | 2871 | 23 | 2 | - | 1753 | 1796 | yes |  |
| 8x4 | anonymous public owners | full+data+ring+program | yes | 2967 | 23 | 2 | - | 1849 | 1892 | yes |  |
| 8x4 | confidential entry | min | yes | 3164 | 16 | 1 | - | 2517 | 2610 | yes |  |
| 8x4 | confidential entry | full | yes | 3823 | 26 | 3 | 3 | 2708 | 2759 | yes |  |
| 8x4 | confidential entry | full+spl | yes | 4191 | 33 | 3 | - | 2840 | 2763 | no | 95 B |
| 8x4 | confidential entry | full+data | yes | 4000 | 26 | 3 | - | 2885 | 2928 | yes |  |
| 8x4 | confidential entry | full+reply | yes | 3823 | 26 | 3 | - | 2708 | 2759 | yes |  |
| 8x4 | confidential entry | full+2tree | yes | 3861 | 27 | 3 | - | 2713 | 2763 | yes |  |
| 8x4 | confidential entry | full+data+ring | yes | 4096 | 26 | 3 | - | 2981 | 3024 | yes |  |
| 8x4 | anonymous entry | min | yes | 3259 | 16 | 1 | - | 2613 | 2707 | yes |  |
| 8x4 | anonymous entry | full | yes | 3726 | 22 | 2 | 3 | 2804 | 2856 | yes |  |
| 8x4 | anonymous entry | full+spl | yes | 4094 | 29 | 2 | - | 2936 | 2860 | yes |  |
| 8x4 | anonymous entry | full+data | yes | 3903 | 22 | 2 | - | 2981 | 3025 | yes |  |
| 8x4 | anonymous entry | full+reply | yes | 4019 | 22 | 2 | - | 3097 | 3147 | yes |  |
| 8x4 | anonymous entry | full+2tree | yes | 3764 | 23 | 2 | - | 2809 | 2860 | yes |  |
| 8x4 | anonymous entry | full+data+ring | yes | 3999 | 22 | 2 | - | 3077 | 3121 | yes |  |
| 8x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x8 | confidential signer | min | yes | 2776 | 16 | 1 | - | 2129 | 2366 | yes |  |
| 8x8 | confidential signer | full | yes | 3435 | 26 | 3 | 7 | 2320 | 2515 | yes |  |
| 8x8 | confidential signer | full+spl | yes | 3803 | 33 | 3 | - | 2452 | 2519 | yes |  |
| 8x8 | confidential signer | full+data | yes | 3756 | 26 | 3 | - | 2641 | 2820 | yes |  |
| 8x8 | confidential signer | full+reply | yes | 3435 | 26 | 3 | - | 2320 | 2515 | yes |  |
| 8x8 | confidential signer | full+2tree | yes | 3473 | 27 | 3 | - | 2325 | 2519 | yes |  |
| 8x8 | confidential signer | full+data+ring | yes | 3980 | 26 | 3 | - | 2865 | 3044 | yes |  |
| 8x8 | confidential signer | full+data+program | yes | 3723 | 23 | 2 | - | 2605 | 2784 | yes |  |
| 8x8 | confidential signer | full+data+ring+program | yes | 3947 | 23 | 2 | - | 2829 | 3008 | yes |  |
| 8x8 | confidential in-proof | min | yes | 2871 | 16 | 1 | - | 2225 | 2463 | yes |  |
| 8x8 | confidential in-proof | full | yes | 3434 | 25 | 2 | 6 | 2416 | 2612 | yes |  |
| 8x8 | confidential in-proof | full+spl | yes | 3802 | 32 | 2 | - | 2548 | 2616 | yes |  |
| 8x8 | confidential in-proof | full+data | yes | 3755 | 25 | 2 | - | 2737 | 2917 | yes |  |
| 8x8 | confidential in-proof | full+reply | yes | 3434 | 25 | 2 | - | 2416 | 2612 | yes |  |
| 8x8 | confidential in-proof | full+2tree | yes | 3472 | 26 | 2 | - | 2421 | 2616 | yes |  |
| 8x8 | confidential in-proof | full+data+ring | yes | 3979 | 25 | 2 | - | 2961 | 3141 | yes |  |
| 8x8 | anonymous hidden | min | yes | 2839 | 16 | 1 | - | 2193 | 2463 | yes |  |
| 8x8 | anonymous hidden | full | yes | 3306 | 22 | 2 | 8 | 2384 | 2612 | yes |  |
| 8x8 | anonymous hidden | full+spl | yes | 3674 | 29 | 2 | - | 2516 | 2616 | yes |  |
| 8x8 | anonymous hidden | full+data | yes | 3627 | 22 | 2 | - | 2705 | 2917 | yes |  |
| 8x8 | anonymous hidden | full+reply | yes | 3987 | 22 | 2 | - | 3065 | 3291 | yes |  |
| 8x8 | anonymous hidden | full+2tree | yes | 3344 | 23 | 2 | - | 2389 | 2616 | yes |  |
| 8x8 | anonymous hidden | full+data+ring | yes | 3851 | 22 | 2 | - | 2929 | 3141 | yes |  |
| 8x8 | anonymous public owners | min | yes | 2776 | 16 | 1 | - | 2129 | 2366 | yes |  |
| 8x8 | anonymous public owners | full | yes | 3435 | 26 | 3 | 7 | 2320 | 2515 | yes |  |
| 8x8 | anonymous public owners | full+spl | yes | 3803 | 33 | 3 | - | 2452 | 2519 | yes |  |
| 8x8 | anonymous public owners | full+data | yes | 3756 | 26 | 3 | - | 2641 | 2820 | yes |  |
| 8x8 | anonymous public owners | full+reply | yes | 4116 | 26 | 3 | - | 3001 | 3194 | no | 20 B |
| 8x8 | anonymous public owners | full+2tree | yes | 3473 | 27 | 3 | - | 2325 | 2519 | yes |  |
| 8x8 | anonymous public owners | full+data+ring | yes | 3980 | 26 | 3 | - | 2865 | 3044 | yes |  |
| 8x8 | anonymous public owners | full+data+program | yes | 3723 | 23 | 2 | - | 2605 | 2784 | yes |  |
| 8x8 | anonymous public owners | full+data+ring+program | yes | 3947 | 23 | 2 | - | 2829 | 3008 | yes |  |
| 8x8 | confidential entry | min | yes | 3872 | 16 | 1 | - | 3225 | 3462 | yes |  |
| 8x8 | confidential entry | full | yes | 4531 | 26 | 3 | none | 3416 | 3611 | no | 435 B |
| 8x8 | confidential entry | full+spl | yes | 4899 | 33 | 3 | - | 3548 | 3615 | no | 803 B |
| 8x8 | confidential entry | full+data | yes | 4852 | 26 | 3 | - | 3737 | 3916 | no | 756 B |
| 8x8 | confidential entry | full+reply | yes | 4531 | 26 | 3 | - | 3416 | 3611 | no | 435 B |
| 8x8 | confidential entry | full+2tree | yes | 4569 | 27 | 3 | - | 3421 | 3615 | no | 473 B |
| 8x8 | confidential entry | full+data+ring | yes | 5076 | 26 | 3 | - | 3961 | 4140 | no | 980 B |
| 8x8 | anonymous entry | min | yes | 3967 | 16 | 1 | - | 3321 | 3559 | yes |  |
| 8x8 | anonymous entry | full | yes | 4434 | 22 | 2 | none | 3512 | 3708 | no | 338 B |
| 8x8 | anonymous entry | full+spl | yes | 4802 | 29 | 2 | - | 3644 | 3712 | no | 706 B |
| 8x8 | anonymous entry | full+data | yes | 4755 | 22 | 2 | - | 3833 | 4013 | no | 659 B |
| 8x8 | anonymous entry | full+reply | yes | 5115 | 22 | 2 | - | 4193 | 4387 | no | 1019 B |
| 8x8 | anonymous entry | full+2tree | yes | 4472 | 23 | 2 | - | 3517 | 3712 | no | 376 B |
| 8x8 | anonymous entry | full+data+ring | yes | 4979 | 22 | 2 | - | 4057 | 4237 | no | 883 B |
| 8x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x16 | confidential signer | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+data+program | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential signer | full+data+ring+program | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential in-proof | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous hidden | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+data+program | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous public owners | full+data+ring+program | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 8x16 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x16 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x2 | confidential signer | min | yes | 1974 | 20 | 1 | - | 1195 | 1220 | yes |  |
| 12x2 | confidential signer | full | yes | 2633 | 30 | 3 | 10 | 1386 | 1369 | yes |  |
| 12x2 | confidential signer | full+spl | yes | 3001 | 37 | 3 | - | 1518 | 1373 | yes |  |
| 12x2 | confidential signer | full+data | yes | 2738 | 30 | 3 | - | 1491 | 1470 | yes |  |
| 12x2 | confidential signer | full+reply | yes | 2633 | 30 | 3 | - | 1386 | 1369 | yes |  |
| 12x2 | confidential signer | full+2tree | yes | 2672 | 31 | 3 | - | 1392 | 1373 | yes |  |
| 12x2 | confidential signer | full+data+ring | yes | 2770 | 30 | 3 | - | 1523 | 1502 | yes |  |
| 12x2 | confidential signer | full+data+program | yes | 2705 | 27 | 2 | - | 1455 | 1434 | yes |  |
| 12x2 | confidential signer | full+data+ring+program | yes | 2737 | 27 | 2 | - | 1487 | 1466 | yes |  |
| 12x2 | confidential in-proof | min | yes | 2069 | 20 | 1 | - | 1291 | 1317 | yes |  |
| 12x2 | confidential in-proof | full | yes | 2632 | 29 | 2 | 10 | 1482 | 1466 | yes |  |
| 12x2 | confidential in-proof | full+spl | yes | 3000 | 36 | 2 | - | 1614 | 1470 | yes |  |
| 12x2 | confidential in-proof | full+data | yes | 2737 | 29 | 2 | - | 1587 | 1567 | yes |  |
| 12x2 | confidential in-proof | full+reply | yes | 2632 | 29 | 2 | - | 1482 | 1466 | yes |  |
| 12x2 | confidential in-proof | full+2tree | yes | 2671 | 30 | 2 | - | 1488 | 1470 | yes |  |
| 12x2 | confidential in-proof | full+data+ring | yes | 2769 | 29 | 2 | - | 1619 | 1599 | yes |  |
| 12x2 | anonymous hidden | min | yes | 2037 | 20 | 1 | - | 1259 | 1317 | yes |  |
| 12x2 | anonymous hidden | full | yes | 2504 | 26 | 2 | 10 | 1450 | 1466 | yes |  |
| 12x2 | anonymous hidden | full+spl | yes | 2872 | 33 | 2 | - | 1582 | 1470 | yes |  |
| 12x2 | anonymous hidden | full+data | yes | 2609 | 26 | 2 | - | 1555 | 1567 | yes |  |
| 12x2 | anonymous hidden | full+reply | yes | 2603 | 26 | 2 | - | 1549 | 1563 | yes |  |
| 12x2 | anonymous hidden | full+2tree | yes | 2543 | 27 | 2 | - | 1456 | 1470 | yes |  |
| 12x2 | anonymous hidden | full+data+ring | yes | 2641 | 26 | 2 | - | 1587 | 1599 | yes |  |
| 12x2 | anonymous public owners | min | yes | 1974 | 20 | 1 | - | 1195 | 1220 | yes |  |
| 12x2 | anonymous public owners | full | yes | 2633 | 30 | 3 | 10 | 1386 | 1369 | yes |  |
| 12x2 | anonymous public owners | full+spl | yes | 3001 | 37 | 3 | - | 1518 | 1373 | yes |  |
| 12x2 | anonymous public owners | full+data | yes | 2738 | 30 | 3 | - | 1491 | 1470 | yes |  |
| 12x2 | anonymous public owners | full+reply | yes | 2732 | 30 | 3 | - | 1485 | 1466 | yes |  |
| 12x2 | anonymous public owners | full+2tree | yes | 2672 | 31 | 3 | - | 1392 | 1373 | yes |  |
| 12x2 | anonymous public owners | full+data+ring | yes | 2770 | 30 | 3 | - | 1523 | 1502 | yes |  |
| 12x2 | anonymous public owners | full+data+program | yes | 2705 | 27 | 2 | - | 1455 | 1434 | yes |  |
| 12x2 | anonymous public owners | full+data+ring+program | yes | 2737 | 27 | 2 | - | 1487 | 1466 | yes |  |
| 12x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 12x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x4 | confidential signer | min | yes | 2328 | 20 | 1 | - | 1549 | 1646 | yes |  |
| 12x4 | confidential signer | full | yes | 2987 | 30 | 3 | 10 | 1740 | 1795 | yes |  |
| 12x4 | confidential signer | full+spl | yes | 3355 | 37 | 3 | - | 1872 | 1799 | yes |  |
| 12x4 | confidential signer | full+data | yes | 3164 | 30 | 3 | - | 1917 | 1964 | yes |  |
| 12x4 | confidential signer | full+reply | yes | 2987 | 30 | 3 | - | 1740 | 1795 | yes |  |
| 12x4 | confidential signer | full+2tree | yes | 3026 | 31 | 3 | - | 1746 | 1799 | yes |  |
| 12x4 | confidential signer | full+data+ring | yes | 3260 | 30 | 3 | - | 2013 | 2060 | yes |  |
| 12x4 | confidential signer | full+data+program | yes | 3131 | 27 | 2 | - | 1881 | 1928 | yes |  |
| 12x4 | confidential signer | full+data+ring+program | yes | 3227 | 27 | 2 | - | 1977 | 2024 | yes |  |
| 12x4 | confidential in-proof | min | yes | 2423 | 20 | 1 | - | 1645 | 1743 | yes |  |
| 12x4 | confidential in-proof | full | yes | 2986 | 29 | 2 | 10 | 1836 | 1892 | yes |  |
| 12x4 | confidential in-proof | full+spl | yes | 3354 | 36 | 2 | - | 1968 | 1896 | yes |  |
| 12x4 | confidential in-proof | full+data | yes | 3163 | 29 | 2 | - | 2013 | 2061 | yes |  |
| 12x4 | confidential in-proof | full+reply | yes | 2986 | 29 | 2 | - | 1836 | 1892 | yes |  |
| 12x4 | confidential in-proof | full+2tree | yes | 3025 | 30 | 2 | - | 1842 | 1896 | yes |  |
| 12x4 | confidential in-proof | full+data+ring | yes | 3259 | 29 | 2 | - | 2109 | 2157 | yes |  |
| 12x4 | anonymous hidden | min | yes | 2391 | 20 | 1 | - | 1613 | 1743 | yes |  |
| 12x4 | anonymous hidden | full | yes | 2858 | 26 | 2 | 10 | 1804 | 1892 | yes |  |
| 12x4 | anonymous hidden | full+spl | yes | 3226 | 33 | 2 | - | 1936 | 1896 | yes |  |
| 12x4 | anonymous hidden | full+data | yes | 3035 | 26 | 2 | - | 1981 | 2061 | yes |  |
| 12x4 | anonymous hidden | full+reply | yes | 3151 | 26 | 2 | - | 2097 | 2183 | yes |  |
| 12x4 | anonymous hidden | full+2tree | yes | 2897 | 27 | 2 | - | 1810 | 1896 | yes |  |
| 12x4 | anonymous hidden | full+data+ring | yes | 3131 | 26 | 2 | - | 2077 | 2157 | yes |  |
| 12x4 | anonymous public owners | min | yes | 2328 | 20 | 1 | - | 1549 | 1646 | yes |  |
| 12x4 | anonymous public owners | full | yes | 2987 | 30 | 3 | 10 | 1740 | 1795 | yes |  |
| 12x4 | anonymous public owners | full+spl | yes | 3355 | 37 | 3 | - | 1872 | 1799 | yes |  |
| 12x4 | anonymous public owners | full+data | yes | 3164 | 30 | 3 | - | 1917 | 1964 | yes |  |
| 12x4 | anonymous public owners | full+reply | yes | 3280 | 30 | 3 | - | 2033 | 2086 | yes |  |
| 12x4 | anonymous public owners | full+2tree | yes | 3026 | 31 | 3 | - | 1746 | 1799 | yes |  |
| 12x4 | anonymous public owners | full+data+ring | yes | 3260 | 30 | 3 | - | 2013 | 2060 | yes |  |
| 12x4 | anonymous public owners | full+data+program | yes | 3131 | 27 | 2 | - | 1881 | 1928 | yes |  |
| 12x4 | anonymous public owners | full+data+ring+program | yes | 3227 | 27 | 2 | - | 1977 | 2024 | yes |  |
| 12x4 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 12x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x8 | confidential signer | min | yes | 3036 | 20 | 1 | - | 2257 | 2498 | yes |  |
| 12x8 | confidential signer | full | yes | 3695 | 30 | 3 | 5 | 2448 | 2647 | yes |  |
| 12x8 | confidential signer | full+spl | yes | 4063 | 37 | 3 | - | 2580 | 2651 | yes |  |
| 12x8 | confidential signer | full+data | yes | 4016 | 30 | 3 | - | 2769 | 2952 | yes |  |
| 12x8 | confidential signer | full+reply | yes | 3695 | 30 | 3 | - | 2448 | 2647 | yes |  |
| 12x8 | confidential signer | full+2tree | yes | 3734 | 31 | 3 | - | 2454 | 2651 | yes |  |
| 12x8 | confidential signer | full+data+ring | yes | 4240 | 30 | 3 | - | 2993 | 3176 | no | 144 B |
| 12x8 | confidential signer | full+data+program | yes | 3983 | 27 | 2 | - | 2733 | 2916 | yes |  |
| 12x8 | confidential signer | full+data+ring+program | yes | 4207 | 27 | 2 | - | 2957 | 3140 | no | 111 B |
| 12x8 | confidential in-proof | min | yes | 3131 | 20 | 1 | - | 2353 | 2595 | yes |  |
| 12x8 | confidential in-proof | full | yes | 3694 | 29 | 2 | 4 | 2544 | 2744 | yes |  |
| 12x8 | confidential in-proof | full+spl | yes | 4062 | 36 | 2 | - | 2676 | 2748 | yes |  |
| 12x8 | confidential in-proof | full+data | yes | 4015 | 29 | 2 | - | 2865 | 3049 | yes |  |
| 12x8 | confidential in-proof | full+reply | yes | 3694 | 29 | 2 | - | 2544 | 2744 | yes |  |
| 12x8 | confidential in-proof | full+2tree | yes | 3733 | 30 | 2 | - | 2550 | 2748 | yes |  |
| 12x8 | confidential in-proof | full+data+ring | yes | 4239 | 29 | 2 | - | 3089 | 3273 | no | 143 B |
| 12x8 | anonymous hidden | min | yes | 3099 | 20 | 1 | - | 2321 | 2595 | yes |  |
| 12x8 | anonymous hidden | full | yes | 3566 | 26 | 2 | 5 | 2512 | 2744 | yes |  |
| 12x8 | anonymous hidden | full+spl | yes | 3934 | 33 | 2 | - | 2644 | 2748 | yes |  |
| 12x8 | anonymous hidden | full+data | yes | 3887 | 26 | 2 | - | 2833 | 3049 | yes |  |
| 12x8 | anonymous hidden | full+reply | yes | 4247 | 26 | 2 | - | 3193 | 3423 | no | 151 B |
| 12x8 | anonymous hidden | full+2tree | yes | 3605 | 27 | 2 | - | 2518 | 2748 | yes |  |
| 12x8 | anonymous hidden | full+data+ring | yes | 4111 | 26 | 2 | - | 3057 | 3273 | no | 15 B |
| 12x8 | anonymous public owners | min | yes | 3036 | 20 | 1 | - | 2257 | 2498 | yes |  |
| 12x8 | anonymous public owners | full | yes | 3695 | 30 | 3 | 5 | 2448 | 2647 | yes |  |
| 12x8 | anonymous public owners | full+spl | yes | 4063 | 37 | 3 | - | 2580 | 2651 | yes |  |
| 12x8 | anonymous public owners | full+data | yes | 4016 | 30 | 3 | - | 2769 | 2952 | yes |  |
| 12x8 | anonymous public owners | full+reply | yes | 4376 | 30 | 3 | - | 3129 | 3326 | no | 280 B |
| 12x8 | anonymous public owners | full+2tree | yes | 3734 | 31 | 3 | - | 2454 | 2651 | yes |  |
| 12x8 | anonymous public owners | full+data+ring | yes | 4240 | 30 | 3 | - | 2993 | 3176 | no | 144 B |
| 12x8 | anonymous public owners | full+data+program | yes | 3983 | 27 | 2 | - | 2733 | 2916 | yes |  |
| 12x8 | anonymous public owners | full+data+ring+program | yes | 4207 | 27 | 2 | - | 2957 | 3140 | no | 111 B |
| 12x8 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 12x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 12x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x2 | confidential signer | min | yes | 2234 | 24 | 1 | - | 1323 | 1352 | yes |  |
| 16x2 | confidential signer | full | yes | 2893 | 34 | 3 | 10 | 1514 | 1501 | yes |  |
| 16x2 | confidential signer | full+spl | yes | 3261 | 41 | 3 | - | 1646 | 1505 | yes |  |
| 16x2 | confidential signer | full+data | yes | 2998 | 34 | 3 | - | 1619 | 1602 | yes |  |
| 16x2 | confidential signer | full+reply | yes | 2893 | 34 | 3 | - | 1514 | 1501 | yes |  |
| 16x2 | confidential signer | full+2tree | yes | 2932 | 35 | 3 | - | 1520 | 1505 | yes |  |
| 16x2 | confidential signer | full+data+ring | yes | 3030 | 34 | 3 | - | 1651 | 1634 | yes |  |
| 16x2 | confidential signer | full+data+program | yes | 2965 | 31 | 2 | - | 1583 | 1566 | yes |  |
| 16x2 | confidential signer | full+data+ring+program | yes | 2997 | 31 | 2 | - | 1615 | 1598 | yes |  |
| 16x2 | confidential in-proof | min | yes | 2329 | 24 | 1 | - | 1419 | 1449 | yes |  |
| 16x2 | confidential in-proof | full | yes | 2892 | 33 | 2 | 10 | 1610 | 1598 | yes |  |
| 16x2 | confidential in-proof | full+spl | yes | 3260 | 40 | 2 | - | 1742 | 1602 | yes |  |
| 16x2 | confidential in-proof | full+data | yes | 2997 | 33 | 2 | - | 1715 | 1699 | yes |  |
| 16x2 | confidential in-proof | full+reply | yes | 2892 | 33 | 2 | - | 1610 | 1598 | yes |  |
| 16x2 | confidential in-proof | full+2tree | yes | 2931 | 34 | 2 | - | 1616 | 1602 | yes |  |
| 16x2 | confidential in-proof | full+data+ring | yes | 3029 | 33 | 2 | - | 1747 | 1731 | yes |  |
| 16x2 | anonymous hidden | min | yes | 2297 | 24 | 1 | - | 1387 | 1449 | yes |  |
| 16x2 | anonymous hidden | full | yes | 2764 | 30 | 2 | 10 | 1578 | 1598 | yes |  |
| 16x2 | anonymous hidden | full+spl | yes | 3132 | 37 | 2 | - | 1710 | 1602 | yes |  |
| 16x2 | anonymous hidden | full+data | yes | 2869 | 30 | 2 | - | 1683 | 1699 | yes |  |
| 16x2 | anonymous hidden | full+reply | yes | 2863 | 30 | 2 | - | 1677 | 1695 | yes |  |
| 16x2 | anonymous hidden | full+2tree | yes | 2803 | 31 | 2 | - | 1584 | 1602 | yes |  |
| 16x2 | anonymous hidden | full+data+ring | yes | 2901 | 30 | 2 | - | 1715 | 1731 | yes |  |
| 16x2 | anonymous public owners | min | yes | 2234 | 24 | 1 | - | 1323 | 1352 | yes |  |
| 16x2 | anonymous public owners | full | yes | 2893 | 34 | 3 | 10 | 1514 | 1501 | yes |  |
| 16x2 | anonymous public owners | full+spl | yes | 3261 | 41 | 3 | - | 1646 | 1505 | yes |  |
| 16x2 | anonymous public owners | full+data | yes | 2998 | 34 | 3 | - | 1619 | 1602 | yes |  |
| 16x2 | anonymous public owners | full+reply | yes | 2992 | 34 | 3 | - | 1613 | 1598 | yes |  |
| 16x2 | anonymous public owners | full+2tree | yes | 2932 | 35 | 3 | - | 1520 | 1505 | yes |  |
| 16x2 | anonymous public owners | full+data+ring | yes | 3030 | 34 | 3 | - | 1651 | 1634 | yes |  |
| 16x2 | anonymous public owners | full+data+program | yes | 2965 | 31 | 2 | - | 1583 | 1566 | yes |  |
| 16x2 | anonymous public owners | full+data+ring+program | yes | 2997 | 31 | 2 | - | 1615 | 1598 | yes |  |
| 16x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 16x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x4 | confidential signer | min | yes | 2588 | 24 | 1 | - | 1677 | 1778 | yes |  |
| 16x4 | confidential signer | full | yes | 3247 | 34 | 3 | 9 | 1868 | 1927 | yes |  |
| 16x4 | confidential signer | full+spl | yes | 3615 | 41 | 3 | - | 2000 | 1931 | yes |  |
| 16x4 | confidential signer | full+data | yes | 3424 | 34 | 3 | - | 2045 | 2096 | yes |  |
| 16x4 | confidential signer | full+reply | yes | 3247 | 34 | 3 | - | 1868 | 1927 | yes |  |
| 16x4 | confidential signer | full+2tree | yes | 3286 | 35 | 3 | - | 1874 | 1931 | yes |  |
| 16x4 | confidential signer | full+data+ring | yes | 3520 | 34 | 3 | - | 2141 | 2192 | yes |  |
| 16x4 | confidential signer | full+data+program | yes | 3391 | 31 | 2 | - | 2009 | 2060 | yes |  |
| 16x4 | confidential signer | full+data+ring+program | yes | 3487 | 31 | 2 | - | 2105 | 2156 | yes |  |
| 16x4 | confidential in-proof | min | yes | 2683 | 24 | 1 | - | 1773 | 1875 | yes |  |
| 16x4 | confidential in-proof | full | yes | 3246 | 33 | 2 | 8 | 1964 | 2024 | yes |  |
| 16x4 | confidential in-proof | full+spl | yes | 3614 | 40 | 2 | - | 2096 | 2028 | yes |  |
| 16x4 | confidential in-proof | full+data | yes | 3423 | 33 | 2 | - | 2141 | 2193 | yes |  |
| 16x4 | confidential in-proof | full+reply | yes | 3246 | 33 | 2 | - | 1964 | 2024 | yes |  |
| 16x4 | confidential in-proof | full+2tree | yes | 3285 | 34 | 2 | - | 1970 | 2028 | yes |  |
| 16x4 | confidential in-proof | full+data+ring | yes | 3519 | 33 | 2 | - | 2237 | 2289 | yes |  |
| 16x4 | anonymous hidden | min | yes | 2651 | 24 | 1 | - | 1741 | 1875 | yes |  |
| 16x4 | anonymous hidden | full | yes | 3118 | 30 | 2 | 10 | 1932 | 2024 | yes |  |
| 16x4 | anonymous hidden | full+spl | yes | 3486 | 37 | 2 | - | 2064 | 2028 | yes |  |
| 16x4 | anonymous hidden | full+data | yes | 3295 | 30 | 2 | - | 2109 | 2193 | yes |  |
| 16x4 | anonymous hidden | full+reply | yes | 3411 | 30 | 2 | - | 2225 | 2315 | yes |  |
| 16x4 | anonymous hidden | full+2tree | yes | 3157 | 31 | 2 | - | 1938 | 2028 | yes |  |
| 16x4 | anonymous hidden | full+data+ring | yes | 3391 | 30 | 2 | - | 2205 | 2289 | yes |  |
| 16x4 | anonymous public owners | min | yes | 2588 | 24 | 1 | - | 1677 | 1778 | yes |  |
| 16x4 | anonymous public owners | full | yes | 3247 | 34 | 3 | 9 | 1868 | 1927 | yes |  |
| 16x4 | anonymous public owners | full+spl | yes | 3615 | 41 | 3 | - | 2000 | 1931 | yes |  |
| 16x4 | anonymous public owners | full+data | yes | 3424 | 34 | 3 | - | 2045 | 2096 | yes |  |
| 16x4 | anonymous public owners | full+reply | yes | 3540 | 34 | 3 | - | 2161 | 2218 | yes |  |
| 16x4 | anonymous public owners | full+2tree | yes | 3286 | 35 | 3 | - | 1874 | 1931 | yes |  |
| 16x4 | anonymous public owners | full+data+ring | yes | 3520 | 34 | 3 | - | 2141 | 2192 | yes |  |
| 16x4 | anonymous public owners | full+data+program | yes | 3391 | 31 | 2 | - | 2009 | 2060 | yes |  |
| 16x4 | anonymous public owners | full+data+ring+program | yes | 3487 | 31 | 2 | - | 2105 | 2156 | yes |  |
| 16x4 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 16x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x8 | confidential signer | min | yes | 3296 | 24 | 1 | - | 2385 | 2630 | yes |  |
| 16x8 | confidential signer | full | yes | 3955 | 34 | 3 | 2 | 2576 | 2779 | yes |  |
| 16x8 | confidential signer | full+spl | yes | 4323 | 41 | 3 | - | 2708 | 2783 | no | 227 B |
| 16x8 | confidential signer | full+data | yes | 4276 | 34 | 3 | - | 2897 | 3084 | no | 180 B |
| 16x8 | confidential signer | full+reply | yes | 3955 | 34 | 3 | - | 2576 | 2779 | yes |  |
| 16x8 | confidential signer | full+2tree | yes | 3994 | 35 | 3 | - | 2582 | 2783 | yes |  |
| 16x8 | confidential signer | full+data+ring | yes | 4500 | 34 | 3 | - | 3121 | 3308 | no | 404 B |
| 16x8 | confidential signer | full+data+program | yes | 4243 | 31 | 2 | - | 2861 | 3048 | no | 147 B |
| 16x8 | confidential signer | full+data+ring+program | yes | 4467 | 31 | 2 | - | 3085 | 3272 | no | 371 B |
| 16x8 | confidential in-proof | min | yes | 3391 | 24 | 1 | - | 2481 | 2727 | yes |  |
| 16x8 | confidential in-proof | full | yes | 3954 | 33 | 2 | 1 | 2672 | 2876 | yes |  |
| 16x8 | confidential in-proof | full+spl | yes | 4322 | 40 | 2 | - | 2804 | 2880 | no | 226 B |
| 16x8 | confidential in-proof | full+data | yes | 4275 | 33 | 2 | - | 2993 | 3181 | no | 179 B |
| 16x8 | confidential in-proof | full+reply | yes | 3954 | 33 | 2 | - | 2672 | 2876 | yes |  |
| 16x8 | confidential in-proof | full+2tree | yes | 3993 | 34 | 2 | - | 2678 | 2880 | yes |  |
| 16x8 | confidential in-proof | full+data+ring | yes | 4499 | 33 | 2 | - | 3217 | 3405 | no | 403 B |
| 16x8 | anonymous hidden | min | yes | 3359 | 24 | 1 | - | 2449 | 2727 | yes |  |
| 16x8 | anonymous hidden | full | yes | 3826 | 30 | 2 | 2 | 2640 | 2876 | yes |  |
| 16x8 | anonymous hidden | full+spl | yes | 4194 | 37 | 2 | - | 2772 | 2880 | no | 98 B |
| 16x8 | anonymous hidden | full+data | yes | 4147 | 30 | 2 | - | 2961 | 3181 | no | 51 B |
| 16x8 | anonymous hidden | full+reply | yes | 4507 | 30 | 2 | - | 3321 | 3555 | no | 411 B |
| 16x8 | anonymous hidden | full+2tree | yes | 3865 | 31 | 2 | - | 2646 | 2880 | yes |  |
| 16x8 | anonymous hidden | full+data+ring | yes | 4371 | 30 | 2 | - | 3185 | 3405 | no | 275 B |
| 16x8 | anonymous public owners | min | yes | 3296 | 24 | 1 | - | 2385 | 2630 | yes |  |
| 16x8 | anonymous public owners | full | yes | 3955 | 34 | 3 | 2 | 2576 | 2779 | yes |  |
| 16x8 | anonymous public owners | full+spl | yes | 4323 | 41 | 3 | - | 2708 | 2783 | no | 227 B |
| 16x8 | anonymous public owners | full+data | yes | 4276 | 34 | 3 | - | 2897 | 3084 | no | 180 B |
| 16x8 | anonymous public owners | full+reply | yes | 4636 | 34 | 3 | - | 3257 | 3458 | no | 540 B |
| 16x8 | anonymous public owners | full+2tree | yes | 3994 | 35 | 3 | - | 2582 | 2783 | yes |  |
| 16x8 | anonymous public owners | full+data+ring | yes | 4500 | 34 | 3 | - | 3121 | 3308 | no | 404 B |
| 16x8 | anonymous public owners | full+data+program | yes | 4243 | 31 | 2 | - | 2861 | 3048 | no | 147 B |
| 16x8 | anonymous public owners | full+data+ring+program | yes | 4467 | 31 | 2 | - | 3085 | 3272 | no | 371 B |
| 16x8 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 16x8 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 16x8 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 24x2 | confidential signer | min | yes | 2754 | 32 | 1 | - | 1579 | 1616 | yes |  |
| 24x2 | confidential signer | full | yes | 3413 | 42 | 3 | 8 | 1770 | 1765 | yes |  |
| 24x2 | confidential signer | full+spl | yes | 3781 | 49 | 3 | - | 1902 | 1769 | yes |  |
| 24x2 | confidential signer | full+data | yes | 3518 | 42 | 3 | - | 1875 | 1866 | yes |  |
| 24x2 | confidential signer | full+reply | yes | 3413 | 42 | 3 | - | 1770 | 1765 | yes |  |
| 24x2 | confidential signer | full+2tree | yes | 3453 | 43 | 3 | - | 1777 | 1769 | yes |  |
| 24x2 | confidential signer | full+data+ring | yes | 3550 | 42 | 3 | - | 1907 | 1898 | yes |  |
| 24x2 | confidential signer | full+data+program | yes | 3485 | 39 | 2 | - | 1839 | 1830 | yes |  |
| 24x2 | confidential signer | full+data+ring+program | yes | 3517 | 39 | 2 | - | 1871 | 1862 | yes |  |
| 24x2 | confidential in-proof | min | yes | 2849 | 32 | 1 | - | 1675 | 1713 | yes |  |
| 24x2 | confidential in-proof | full | yes | 3412 | 41 | 2 | 7 | 1866 | 1862 | yes |  |
| 24x2 | confidential in-proof | full+spl | yes | 3780 | 48 | 2 | - | 1998 | 1866 | yes |  |
| 24x2 | confidential in-proof | full+data | yes | 3517 | 41 | 2 | - | 1971 | 1963 | yes |  |
| 24x2 | confidential in-proof | full+reply | yes | 3412 | 41 | 2 | - | 1866 | 1862 | yes |  |
| 24x2 | confidential in-proof | full+2tree | yes | 3452 | 42 | 2 | - | 1873 | 1866 | yes |  |
| 24x2 | confidential in-proof | full+data+ring | yes | 3549 | 41 | 2 | - | 2003 | 1995 | yes |  |
| 24x2 | anonymous hidden | min | yes | 2817 | 32 | 1 | - | 1643 | 1713 | yes |  |
| 24x2 | anonymous hidden | full | yes | 3284 | 38 | 2 | 8 | 1834 | 1862 | yes |  |
| 24x2 | anonymous hidden | full+spl | yes | 3652 | 45 | 2 | - | 1966 | 1866 | yes |  |
| 24x2 | anonymous hidden | full+data | yes | 3389 | 38 | 2 | - | 1939 | 1963 | yes |  |
| 24x2 | anonymous hidden | full+reply | yes | 3383 | 38 | 2 | - | 1933 | 1959 | yes |  |
| 24x2 | anonymous hidden | full+2tree | yes | 3324 | 39 | 2 | - | 1841 | 1866 | yes |  |
| 24x2 | anonymous hidden | full+data+ring | yes | 3421 | 38 | 2 | - | 1971 | 1995 | yes |  |
| 24x2 | anonymous public owners | min | yes | 2754 | 32 | 1 | - | 1579 | 1616 | yes |  |
| 24x2 | anonymous public owners | full | yes | 3413 | 42 | 3 | 8 | 1770 | 1765 | yes |  |
| 24x2 | anonymous public owners | full+spl | yes | 3781 | 49 | 3 | - | 1902 | 1769 | yes |  |
| 24x2 | anonymous public owners | full+data | yes | 3518 | 42 | 3 | - | 1875 | 1866 | yes |  |
| 24x2 | anonymous public owners | full+reply | yes | 3512 | 42 | 3 | - | 1869 | 1862 | yes |  |
| 24x2 | anonymous public owners | full+2tree | yes | 3453 | 43 | 3 | - | 1777 | 1769 | yes |  |
| 24x2 | anonymous public owners | full+data+ring | yes | 3550 | 42 | 3 | - | 1907 | 1898 | yes |  |
| 24x2 | anonymous public owners | full+data+program | yes | 3485 | 39 | 2 | - | 1839 | 1830 | yes |  |
| 24x2 | anonymous public owners | full+data+ring+program | yes | 3517 | 39 | 2 | - | 1871 | 1862 | yes |  |
| 24x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 24x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 24x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 24x4 | confidential signer | min | yes | 3108 | 32 | 1 | - | 1933 | 2042 | yes |  |
| 24x4 | confidential signer | full | yes | 3767 | 42 | 3 | 4 | 2124 | 2191 | yes |  |
| 24x4 | confidential signer | full+spl | yes | 4135 | 49 | 3 | - | 2256 | 2195 | no | 39 B |
| 24x4 | confidential signer | full+data | yes | 3944 | 42 | 3 | - | 2301 | 2360 | yes |  |
| 24x4 | confidential signer | full+reply | yes | 3767 | 42 | 3 | - | 2124 | 2191 | yes |  |
| 24x4 | confidential signer | full+2tree | yes | 3807 | 43 | 3 | - | 2131 | 2195 | yes |  |
| 24x4 | confidential signer | full+data+ring | yes | 4040 | 42 | 3 | - | 2397 | 2456 | yes |  |
| 24x4 | confidential signer | full+data+program | yes | 3911 | 39 | 2 | - | 2265 | 2324 | yes |  |
| 24x4 | confidential signer | full+data+ring+program | yes | 4007 | 39 | 2 | - | 2361 | 2420 | yes |  |
| 24x4 | confidential in-proof | min | yes | 3203 | 32 | 1 | - | 2029 | 2139 | yes |  |
| 24x4 | confidential in-proof | full | yes | 3766 | 41 | 2 | 3 | 2220 | 2288 | yes |  |
| 24x4 | confidential in-proof | full+spl | yes | 4134 | 48 | 2 | - | 2352 | 2292 | no | 38 B |
| 24x4 | confidential in-proof | full+data | yes | 3943 | 41 | 2 | - | 2397 | 2457 | yes |  |
| 24x4 | confidential in-proof | full+reply | yes | 3766 | 41 | 2 | - | 2220 | 2288 | yes |  |
| 24x4 | confidential in-proof | full+2tree | yes | 3806 | 42 | 2 | - | 2227 | 2292 | yes |  |
| 24x4 | confidential in-proof | full+data+ring | yes | 4039 | 41 | 2 | - | 2493 | 2553 | yes |  |
| 24x4 | anonymous hidden | min | yes | 3171 | 32 | 1 | - | 1997 | 2139 | yes |  |
| 24x4 | anonymous hidden | full | yes | 3638 | 38 | 2 | 4 | 2188 | 2288 | yes |  |
| 24x4 | anonymous hidden | full+spl | yes | 4006 | 45 | 2 | - | 2320 | 2292 | yes |  |
| 24x4 | anonymous hidden | full+data | yes | 3815 | 38 | 2 | - | 2365 | 2457 | yes |  |
| 24x4 | anonymous hidden | full+reply | yes | 3931 | 38 | 2 | - | 2481 | 2579 | yes |  |
| 24x4 | anonymous hidden | full+2tree | yes | 3678 | 39 | 2 | - | 2195 | 2292 | yes |  |
| 24x4 | anonymous hidden | full+data+ring | yes | 3911 | 38 | 2 | - | 2461 | 2553 | yes |  |
| 24x4 | anonymous public owners | min | yes | 3108 | 32 | 1 | - | 1933 | 2042 | yes |  |
| 24x4 | anonymous public owners | full | yes | 3767 | 42 | 3 | 4 | 2124 | 2191 | yes |  |
| 24x4 | anonymous public owners | full+spl | yes | 4135 | 49 | 3 | - | 2256 | 2195 | no | 39 B |
| 24x4 | anonymous public owners | full+data | yes | 3944 | 42 | 3 | - | 2301 | 2360 | yes |  |
| 24x4 | anonymous public owners | full+reply | yes | 4060 | 42 | 3 | - | 2417 | 2482 | yes |  |
| 24x4 | anonymous public owners | full+2tree | yes | 3807 | 43 | 3 | - | 2131 | 2195 | yes |  |
| 24x4 | anonymous public owners | full+data+ring | yes | 4040 | 42 | 3 | - | 2397 | 2456 | yes |  |
| 24x4 | anonymous public owners | full+data+program | yes | 3911 | 39 | 2 | - | 2265 | 2324 | yes |  |
| 24x4 | anonymous public owners | full+data+ring+program | yes | 4007 | 39 | 2 | - | 2361 | 2420 | yes |  |
| 24x4 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 24x4 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 24x4 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 32x2 | confidential signer | min | yes | 3274 | 40 | 1 | - | 1835 | 1880 | yes |  |
| 32x2 | confidential signer | full | yes | 3933 | 50 | 3 | 2 | 2026 | 2029 | yes |  |
| 32x2 | confidential signer | full+spl | yes | 4301 | 57 | 3 | - | 2158 | 2033 | no | 205 B |
| 32x2 | confidential signer | full+data | yes | 4038 | 50 | 3 | - | 2131 | 2130 | yes |  |
| 32x2 | confidential signer | full+reply | yes | 3933 | 50 | 3 | - | 2026 | 2029 | yes |  |
| 32x2 | confidential signer | full+2tree | yes | 3974 | 51 | 3 | - | 2034 | 2033 | yes |  |
| 32x2 | confidential signer | full+data+ring | yes | 4070 | 50 | 3 | - | 2163 | 2162 | yes |  |
| 32x2 | confidential signer | full+data+program | yes | 4005 | 47 | 2 | - | 2095 | 2094 | yes |  |
| 32x2 | confidential signer | full+data+ring+program | yes | 4037 | 47 | 2 | - | 2127 | 2126 | yes |  |
| 32x2 | confidential in-proof | min | yes | 3369 | 40 | 1 | - | 1931 | 1977 | yes |  |
| 32x2 | confidential in-proof | full | yes | 3932 | 49 | 2 | 1 | 2122 | 2126 | yes |  |
| 32x2 | confidential in-proof | full+spl | yes | 4300 | 56 | 2 | - | 2254 | 2130 | no | 204 B |
| 32x2 | confidential in-proof | full+data | yes | 4037 | 49 | 2 | - | 2227 | 2227 | yes |  |
| 32x2 | confidential in-proof | full+reply | yes | 3932 | 49 | 2 | - | 2122 | 2126 | yes |  |
| 32x2 | confidential in-proof | full+2tree | yes | 3973 | 50 | 2 | - | 2130 | 2130 | yes |  |
| 32x2 | confidential in-proof | full+data+ring | yes | 4069 | 49 | 2 | - | 2259 | 2259 | yes |  |
| 32x2 | anonymous hidden | min | yes | 3337 | 40 | 1 | - | 1899 | 1977 | yes |  |
| 32x2 | anonymous hidden | full | yes | 3804 | 46 | 2 | 3 | 2090 | 2126 | yes |  |
| 32x2 | anonymous hidden | full+spl | yes | 4172 | 53 | 2 | - | 2222 | 2130 | no | 76 B |
| 32x2 | anonymous hidden | full+data | yes | 3909 | 46 | 2 | - | 2195 | 2227 | yes |  |
| 32x2 | anonymous hidden | full+reply | yes | 3903 | 46 | 2 | - | 2189 | 2223 | yes |  |
| 32x2 | anonymous hidden | full+2tree | yes | 3845 | 47 | 2 | - | 2098 | 2130 | yes |  |
| 32x2 | anonymous hidden | full+data+ring | yes | 3941 | 46 | 2 | - | 2227 | 2259 | yes |  |
| 32x2 | anonymous public owners | min | yes | 3274 | 40 | 1 | - | 1835 | 1880 | yes |  |
| 32x2 | anonymous public owners | full | yes | 3933 | 50 | 3 | 2 | 2026 | 2029 | yes |  |
| 32x2 | anonymous public owners | full+spl | yes | 4301 | 57 | 3 | - | 2158 | 2033 | no | 205 B |
| 32x2 | anonymous public owners | full+data | yes | 4038 | 50 | 3 | - | 2131 | 2130 | yes |  |
| 32x2 | anonymous public owners | full+reply | yes | 4032 | 50 | 3 | - | 2125 | 2126 | yes |  |
| 32x2 | anonymous public owners | full+2tree | yes | 3974 | 51 | 3 | - | 2034 | 2033 | yes |  |
| 32x2 | anonymous public owners | full+data+ring | yes | 4070 | 50 | 3 | - | 2163 | 2162 | yes |  |
| 32x2 | anonymous public owners | full+data+program | yes | 4005 | 47 | 2 | - | 2095 | 2094 | yes |  |
| 32x2 | anonymous public owners | full+data+ring+program | yes | 4037 | 47 | 2 | - | 2127 | 2126 | yes |  |
| 32x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 32x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 32x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 40x2 | confidential signer | min | yes | 3794 | 48 | 1 | - | 2091 | 2144 | yes |  |
| 40x2 | confidential signer | full | yes | 4453 | 58 | 3 | none | 2282 | 2293 | no | 357 B |
| 40x2 | confidential signer | full+spl | yes | 4821 | 65 | 3 | - | 2414 | 2297 | no | 725 B, 1 accounts |
| 40x2 | confidential signer | full+data | yes | 4558 | 58 | 3 | - | 2387 | 2394 | no | 462 B |
| 40x2 | confidential signer | full+reply | yes | 4453 | 58 | 3 | - | 2282 | 2293 | no | 357 B |
| 40x2 | confidential signer | full+2tree | yes | 4495 | 59 | 3 | - | 2291 | 2297 | no | 399 B |
| 40x2 | confidential signer | full+data+ring | yes | 4590 | 58 | 3 | - | 2419 | 2426 | no | 494 B |
| 40x2 | confidential signer | full+data+program | yes | 4525 | 55 | 2 | - | 2351 | 2358 | no | 429 B |
| 40x2 | confidential signer | full+data+ring+program | yes | 4557 | 55 | 2 | - | 2383 | 2390 | no | 461 B |
| 40x2 | confidential in-proof | min | yes | 3889 | 48 | 1 | - | 2187 | 2241 | yes |  |
| 40x2 | confidential in-proof | full | yes | 4452 | 57 | 2 | none | 2378 | 2390 | no | 356 B |
| 40x2 | confidential in-proof | full+spl | yes | 4820 | 64 | 2 | - | 2510 | 2394 | no | 724 B |
| 40x2 | confidential in-proof | full+data | yes | 4557 | 57 | 2 | - | 2483 | 2491 | no | 461 B |
| 40x2 | confidential in-proof | full+reply | yes | 4452 | 57 | 2 | - | 2378 | 2390 | no | 356 B |
| 40x2 | confidential in-proof | full+2tree | yes | 4494 | 58 | 2 | - | 2387 | 2394 | no | 398 B |
| 40x2 | confidential in-proof | full+data+ring | yes | 4589 | 57 | 2 | - | 2515 | 2523 | no | 493 B |
| 40x2 | anonymous hidden | min | yes | 3857 | 48 | 1 | - | 2155 | 2241 | yes |  |
| 40x2 | anonymous hidden | full | yes | 4324 | 54 | 2 | none | 2346 | 2390 | no | 228 B |
| 40x2 | anonymous hidden | full+spl | yes | 4692 | 61 | 2 | - | 2478 | 2394 | no | 596 B |
| 40x2 | anonymous hidden | full+data | yes | 4429 | 54 | 2 | - | 2451 | 2491 | no | 333 B |
| 40x2 | anonymous hidden | full+reply | yes | 4423 | 54 | 2 | - | 2445 | 2487 | no | 327 B |
| 40x2 | anonymous hidden | full+2tree | yes | 4366 | 55 | 2 | - | 2355 | 2394 | no | 270 B |
| 40x2 | anonymous hidden | full+data+ring | yes | 4461 | 54 | 2 | - | 2483 | 2523 | no | 365 B |
| 40x2 | anonymous public owners | min | yes | 3794 | 48 | 1 | - | 2091 | 2144 | yes |  |
| 40x2 | anonymous public owners | full | yes | 4453 | 58 | 3 | none | 2282 | 2293 | no | 357 B |
| 40x2 | anonymous public owners | full+spl | yes | 4821 | 65 | 3 | - | 2414 | 2297 | no | 725 B, 1 accounts |
| 40x2 | anonymous public owners | full+data | yes | 4558 | 58 | 3 | - | 2387 | 2394 | no | 462 B |
| 40x2 | anonymous public owners | full+reply | yes | 4552 | 58 | 3 | - | 2381 | 2390 | no | 456 B |
| 40x2 | anonymous public owners | full+2tree | yes | 4495 | 59 | 3 | - | 2291 | 2297 | no | 399 B |
| 40x2 | anonymous public owners | full+data+ring | yes | 4590 | 58 | 3 | - | 2419 | 2426 | no | 494 B |
| 40x2 | anonymous public owners | full+data+program | yes | 4525 | 55 | 2 | - | 2351 | 2358 | no | 429 B |
| 40x2 | anonymous public owners | full+data+ring+program | yes | 4557 | 55 | 2 | - | 2383 | 2390 | no | 461 B |
| 40x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 40x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 40x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 48x2 | confidential signer | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+data+program | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential signer | full+data+ring+program | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential in-proof | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous hidden | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+data+program | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous public owners | full+data+ring+program | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 48x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 48x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 49x2 | confidential signer | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+data+program | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential signer | full+data+ring+program | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential in-proof | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous hidden | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+data+program | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous public owners | full+data+ring+program | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | confidential entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | min | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+spl | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+data | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+reply | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+2tree | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | anonymous entry | full+data+ring | none | - | - | - | - | - | - | no | catalog key |
| 49x2 | delegate | - | - | - | - | - | - | - | - | no | rail |
| 49x2 | anonymous delegate | - | - | - | - | - | - | - | - | no | rail |
| 8x1 | merge | ring | yes | 1728 | 16 | 1 | - | - | - | yes |  |
| 24x1 | merge | ring | yes | 2768 | 32 | 1 | - | - | - | yes |  |
| 52x1 | merge | ring | yes | 4588 | 60 | 1 | - | - | - | no | 492 B |

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
| 1x2 | confidential signer | messages | 56 | 3146 | 3079 | 4063 | 20 | 3 | 1 B |
| 1x2 | confidential signer | message bytes | 1903 | 3179 | 3112 | 4096 | 20 | 3 | 1 B |
| 1x2 | confidential in-proof | messages | 57 | 3276 | 3210 | 4096 | 19 | 2 | 34 B |
| 1x2 | confidential in-proof | message bytes | 1904 | 3276 | 3210 | 4096 | 19 | 2 | 1 B |
| 1x2 | anonymous hidden | reply | 2065 | 3372 | 3337 | 4096 | 16 | 2 | 1 B |
| 1x2 | anonymous hidden | messages | 57 | 3343 | 3307 | 4067 | 16 | 2 | 5 B |
| 1x2 | anonymous hidden | message bytes | 1933 | 3372 | 3336 | 4096 | 16 | 2 | 1 B |
| 1x2 | anonymous public owners | reply | 1936 | 3179 | 3111 | 4096 | 20 | 3 | 1 B |
| 1x2 | anonymous public owners | messages | 54 | 3177 | 3108 | 4094 | 20 | 3 | 32 B |
| 1x2 | anonymous public owners | message bytes | 1804 | 3179 | 3110 | 4096 | 20 | 3 | 1 B |
| 1x2 | confidential entry | messages | 52 | 3154 | 3087 | 4071 | 20 | 3 | 9 B |
| 1x2 | confidential entry | message bytes | 1759 | 3179 | 3112 | 4096 | 20 | 3 | 1 B |
| 1x2 | anonymous entry | reply | 1889 | 3372 | 3305 | 4096 | 16 | 2 | 1 B |
| 1x2 | anonymous entry | messages | 52 | 3349 | 3281 | 4073 | 16 | 2 | 11 B |
| 1x2 | anonymous entry | message bytes | 1757 | 3372 | 3304 | 4096 | 16 | 2 | 1 B |
| 1x4 | confidential signer | messages | 44 | 3164 | 3165 | 4081 | 20 | 3 | 19 B |
| 1x4 | confidential signer | message bytes | 1477 | 3179 | 3180 | 4096 | 20 | 3 | 1 B |
| 1x4 | confidential in-proof | messages | 44 | 3260 | 3262 | 4080 | 19 | 2 | 18 B |
| 1x4 | confidential in-proof | message bytes | 1478 | 3276 | 3278 | 4096 | 19 | 2 | 1 B |
| 1x4 | anonymous hidden | reply | 546 | 3371 | 3404 | 4095 | 16 | 2 | 2 B |
| 1x4 | anonymous hidden | messages | 39 | 3351 | 3383 | 4075 | 16 | 2 | 13 B |
| 1x4 | anonymous hidden | message bytes | 1313 | 3372 | 3404 | 4096 | 16 | 2 | 1 B |
| 1x4 | anonymous public owners | reply | 503 | 3178 | 3178 | 4095 | 20 | 3 | 2 B |
| 1x4 | anonymous public owners | messages | 35 | 3151 | 3150 | 4068 | 20 | 3 | 6 B |
| 1x4 | anonymous public owners | message bytes | 1184 | 3179 | 3178 | 4096 | 20 | 3 | 1 B |
| 1x4 | confidential entry | messages | 40 | 3172 | 3173 | 4089 | 20 | 3 | 27 B |
| 1x4 | confidential entry | message bytes | 1333 | 3179 | 3180 | 4096 | 20 | 3 | 1 B |
| 1x4 | anonymous entry | reply | 487 | 3370 | 3371 | 4094 | 16 | 2 | 1 B |
| 1x4 | anonymous entry | messages | 34 | 3357 | 3357 | 4081 | 16 | 2 | 19 B |
| 1x4 | anonymous entry | message bytes | 1137 | 3372 | 3372 | 4096 | 16 | 2 | 1 B |
| 1x8 | confidential signer | messages | 19 | 3166 | 3303 | 4083 | 20 | 3 | 21 B |
| 1x8 | confidential signer | message bytes | 625 | 3179 | 3316 | 4096 | 20 | 3 | 1 B |
| 1x8 | confidential in-proof | messages | 19 | 3262 | 3400 | 4082 | 19 | 2 | 20 B |
| 1x8 | confidential in-proof | message bytes | 626 | 3276 | 3414 | 4096 | 19 | 2 | 1 B |
| 1x8 | anonymous hidden | reply | 112 | 3369 | 3538 | 4093 | 16 | 2 | 4 B |
| 1x8 | anonymous hidden | messages | 3 | 3367 | 3535 | 4091 | 16 | 2 | 29 B |
| 1x8 | anonymous hidden | message bytes | 73 | 3372 | 3540 | 4096 | 16 | 2 | 1 B |
| 1x8 | anonymous public owners | reply | none | - | - | - | - | - | 21 B |
| 1x8 | anonymous public owners | messages | none | - | - | - | - | - | 21 B |
| 1x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 56 B |
| 1x8 | confidential entry | messages | 15 | 3174 | 3311 | 4091 | 20 | 3 | 29 B |
| 1x8 | confidential entry | message bytes | 481 | 3179 | 3316 | 4096 | 20 | 3 | 1 B |
| 1x8 | anonymous entry | reply | none | - | - | - | - | - | 68 B |
| 1x8 | anonymous entry | messages | none | - | - | - | - | - | 68 B |
| 1x8 | anonymous entry | message bytes | none | - | - | - | - | - | 103 B |
| 2x2 | confidential signer | messages | 55 | 3144 | 3078 | 4094 | 21 | 3 | 32 B |
| 2x2 | confidential signer | message bytes | 1838 | 3146 | 3080 | 4096 | 21 | 3 | 1 B |
| 2x2 | confidential in-proof | messages | 55 | 3240 | 3175 | 4093 | 20 | 2 | 31 B |
| 2x2 | confidential in-proof | message bytes | 1839 | 3243 | 3178 | 4096 | 20 | 2 | 1 B |
| 2x2 | anonymous hidden | reply | 2000 | 3339 | 3305 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous hidden | messages | 55 | 3307 | 3272 | 4064 | 17 | 2 | 2 B |
| 2x2 | anonymous hidden | message bytes | 1868 | 3339 | 3304 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous public owners | reply | 1871 | 3146 | 3079 | 4096 | 21 | 3 | 1 B |
| 2x2 | anonymous public owners | messages | 52 | 3141 | 3073 | 4091 | 21 | 3 | 29 B |
| 2x2 | anonymous public owners | message bytes | 1739 | 3146 | 3078 | 4096 | 21 | 3 | 1 B |
| 2x2 | confidential entry | messages | 46 | 3118 | 3052 | 4068 | 21 | 3 | 6 B |
| 2x2 | confidential entry | message bytes | 1558 | 3146 | 3080 | 4096 | 21 | 3 | 1 B |
| 2x2 | anonymous entry | reply | 1688 | 3339 | 3273 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous entry | messages | 46 | 3313 | 3246 | 4070 | 17 | 2 | 8 B |
| 2x2 | anonymous entry | message bytes | 1556 | 3339 | 3272 | 4096 | 17 | 2 | 1 B |
| 2x2 | delegate | messages | 62 | 3334 | 3316 | 4088 | 17 | 2 | 26 B |
| 2x2 | delegate | message bytes | 2082 | 3342 | 3324 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous delegate | reply | 2115 | 3342 | 3323 | 4096 | 17 | 2 | 1 B |
| 2x2 | anonymous delegate | messages | 59 | 3331 | 3311 | 4085 | 17 | 2 | 23 B |
| 2x2 | anonymous delegate | message bytes | 1983 | 3342 | 3322 | 4096 | 17 | 2 | 1 B |
| 2x4 | confidential signer | messages | 42 | 3128 | 3130 | 4078 | 21 | 3 | 16 B |
| 2x4 | confidential signer | message bytes | 1412 | 3146 | 3148 | 4096 | 21 | 3 | 1 B |
| 2x4 | confidential in-proof | messages | 42 | 3224 | 3227 | 4077 | 20 | 2 | 15 B |
| 2x4 | confidential in-proof | message bytes | 1413 | 3243 | 3246 | 4096 | 20 | 2 | 1 B |
| 2x4 | anonymous hidden | reply | 524 | 3337 | 3371 | 4094 | 17 | 2 | 1 B |
| 2x4 | anonymous hidden | messages | 37 | 3315 | 3348 | 4072 | 17 | 2 | 10 B |
| 2x4 | anonymous hidden | message bytes | 1248 | 3339 | 3372 | 4096 | 17 | 2 | 1 B |
| 2x4 | anonymous public owners | reply | 481 | 3144 | 3145 | 4094 | 21 | 3 | 1 B |
| 2x4 | anonymous public owners | messages | 33 | 3115 | 3115 | 4065 | 21 | 3 | 3 B |
| 2x4 | anonymous public owners | message bytes | 1119 | 3146 | 3146 | 4096 | 21 | 3 | 1 B |
| 2x4 | confidential entry | messages | 34 | 3136 | 3138 | 4086 | 21 | 3 | 24 B |
| 2x4 | confidential entry | message bytes | 1132 | 3146 | 3148 | 4096 | 21 | 3 | 1 B |
| 2x4 | anonymous entry | reply | 420 | 3337 | 3339 | 4094 | 17 | 2 | 1 B |
| 2x4 | anonymous entry | messages | 28 | 3321 | 3322 | 4078 | 17 | 2 | 16 B |
| 2x4 | anonymous entry | message bytes | 936 | 3339 | 3340 | 4096 | 17 | 2 | 1 B |
| 1x16 | confidential signer | messages | none | - | - | - | - | - | 1045 B |
| 1x16 | confidential signer | message bytes | none | - | - | - | - | - | 1080 B |
| 1x16 | confidential in-proof | messages | none | - | - | - | - | - | 1044 B |
| 1x16 | confidential in-proof | message bytes | none | - | - | - | - | - | 1079 B |
| 1x16 | anonymous hidden | reply | none | - | - | - | - | - | 2373 B |
| 1x16 | anonymous hidden | messages | none | - | - | - | - | - | 2373 B |
| 1x16 | anonymous hidden | message bytes | none | - | - | - | - | - | 2408 B |
| 1x16 | anonymous public owners | reply | none | - | - | - | - | - | 2502 B |
| 1x16 | anonymous public owners | messages | none | - | - | - | - | - | 2502 B |
| 1x16 | anonymous public owners | message bytes | none | - | - | - | - | - | 2537 B |
| 2x8 | confidential signer | messages | 17 | 3130 | 3268 | 4080 | 21 | 3 | 18 B |
| 2x8 | confidential signer | message bytes | 560 | 3146 | 3284 | 4096 | 21 | 3 | 1 B |
| 2x8 | confidential in-proof | messages | 17 | 3226 | 3365 | 4079 | 20 | 2 | 17 B |
| 2x8 | confidential in-proof | message bytes | 561 | 3243 | 3382 | 4096 | 20 | 2 | 1 B |
| 2x8 | anonymous hidden | reply | 103 | 3338 | 3508 | 4095 | 17 | 2 | 6 B |
| 2x8 | anonymous hidden | messages | 1 | 3331 | 3500 | 4088 | 17 | 2 | 26 B |
| 2x8 | anonymous hidden | message bytes | 8 | 3339 | 3508 | 4096 | 17 | 2 | 1 B |
| 2x8 | anonymous public owners | reply | none | - | - | - | - | - | 86 B |
| 2x8 | anonymous public owners | messages | none | - | - | - | - | - | 86 B |
| 2x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 121 B |
| 2x8 | confidential entry | messages | 9 | 3138 | 3276 | 4088 | 21 | 3 | 26 B |
| 2x8 | confidential entry | message bytes | 280 | 3146 | 3284 | 4096 | 21 | 3 | 1 B |
| 2x8 | anonymous entry | reply | none | - | - | - | - | - | 269 B |
| 2x8 | anonymous entry | messages | none | - | - | - | - | - | 269 B |
| 2x8 | anonymous entry | message bytes | none | - | - | - | - | - | 304 B |
| 3x2 | confidential signer | messages | 53 | 3108 | 3043 | 4091 | 22 | 3 | 29 B |
| 3x2 | confidential signer | message bytes | 1773 | 3113 | 3048 | 4096 | 22 | 3 | 1 B |
| 3x2 | confidential in-proof | messages | 53 | 3204 | 3140 | 4090 | 21 | 2 | 28 B |
| 3x2 | confidential in-proof | message bytes | 1774 | 3210 | 3146 | 4096 | 21 | 2 | 1 B |
| 3x2 | anonymous hidden | reply | 1935 | 3306 | 3273 | 4096 | 18 | 2 | 1 B |
| 3x2 | anonymous hidden | messages | 54 | 3305 | 3271 | 4095 | 18 | 2 | 33 B |
| 3x2 | anonymous hidden | message bytes | 1803 | 3306 | 3272 | 4096 | 18 | 2 | 1 B |
| 3x2 | anonymous public owners | reply | 1806 | 3113 | 3047 | 4096 | 22 | 3 | 1 B |
| 3x2 | anonymous public owners | messages | 50 | 3105 | 3038 | 4088 | 22 | 3 | 26 B |
| 3x2 | anonymous public owners | message bytes | 1674 | 3113 | 3046 | 4096 | 22 | 3 | 1 B |
| 3x2 | confidential entry | messages | 40 | 3082 | 3017 | 4065 | 22 | 3 | 3 B |
| 3x2 | confidential entry | message bytes | 1357 | 3113 | 3048 | 4096 | 22 | 3 | 1 B |
| 3x2 | anonymous entry | reply | 1487 | 3306 | 3241 | 4096 | 18 | 2 | 1 B |
| 3x2 | anonymous entry | messages | 40 | 3277 | 3211 | 4067 | 18 | 2 | 5 B |
| 3x2 | anonymous entry | message bytes | 1355 | 3306 | 3240 | 4096 | 18 | 2 | 1 B |
| 3x4 | confidential signer | messages | 40 | 3092 | 3095 | 4075 | 22 | 3 | 13 B |
| 3x4 | confidential signer | message bytes | 1347 | 3113 | 3116 | 4096 | 22 | 3 | 1 B |
| 3x4 | confidential in-proof | messages | 40 | 3188 | 3192 | 4074 | 21 | 2 | 12 B |
| 3x4 | confidential in-proof | message bytes | 1348 | 3210 | 3214 | 4096 | 21 | 2 | 1 B |
| 3x4 | anonymous hidden | reply | 503 | 3306 | 3341 | 4096 | 18 | 2 | 3 B |
| 3x4 | anonymous hidden | messages | 35 | 3279 | 3313 | 4069 | 18 | 2 | 7 B |
| 3x4 | anonymous hidden | message bytes | 1183 | 3306 | 3340 | 4096 | 18 | 2 | 1 B |
| 3x4 | anonymous public owners | reply | 460 | 3113 | 3115 | 4096 | 22 | 3 | 3 B |
| 3x4 | anonymous public owners | messages | 32 | 3113 | 3114 | 4096 | 22 | 3 | 34 B |
| 3x4 | anonymous public owners | message bytes | 1054 | 3113 | 3114 | 4096 | 22 | 3 | 1 B |
| 3x4 | confidential entry | messages | 28 | 3100 | 3103 | 4083 | 22 | 3 | 21 B |
| 3x4 | confidential entry | message bytes | 931 | 3113 | 3116 | 4096 | 22 | 3 | 1 B |
| 3x4 | anonymous entry | reply | 353 | 3304 | 3307 | 4094 | 18 | 2 | 1 B |
| 3x4 | anonymous entry | messages | 22 | 3285 | 3287 | 4075 | 18 | 2 | 13 B |
| 3x4 | anonymous entry | message bytes | 735 | 3306 | 3308 | 4096 | 18 | 2 | 1 B |
| 2x16 | confidential signer | messages | none | - | - | - | - | - | 1110 B |
| 2x16 | confidential signer | message bytes | none | - | - | - | - | - | 1145 B |
| 2x16 | confidential in-proof | messages | none | - | - | - | - | - | 1109 B |
| 2x16 | confidential in-proof | message bytes | none | - | - | - | - | - | 1144 B |
| 2x16 | anonymous hidden | reply | none | - | - | - | - | - | 2438 B |
| 2x16 | anonymous hidden | messages | none | - | - | - | - | - | 2438 B |
| 2x16 | anonymous hidden | message bytes | none | - | - | - | - | - | 2473 B |
| 2x16 | anonymous public owners | reply | none | - | - | - | - | - | 2567 B |
| 2x16 | anonymous public owners | messages | none | - | - | - | - | - | 2567 B |
| 2x16 | anonymous public owners | message bytes | none | - | - | - | - | - | 2602 B |
| 3x8 | confidential signer | messages | 15 | 3094 | 3233 | 4077 | 22 | 3 | 15 B |
| 3x8 | confidential signer | message bytes | 495 | 3113 | 3252 | 4096 | 22 | 3 | 1 B |
| 3x8 | confidential in-proof | messages | 15 | 3190 | 3330 | 4076 | 21 | 2 | 14 B |
| 3x8 | confidential in-proof | message bytes | 496 | 3210 | 3350 | 4096 | 21 | 2 | 1 B |
| 3x8 | anonymous hidden | reply | none | - | - | - | - | - | 22 B |
| 3x8 | anonymous hidden | messages | none | - | - | - | - | - | 22 B |
| 3x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 57 B |
| 3x8 | anonymous public owners | reply | none | - | - | - | - | - | 151 B |
| 3x8 | anonymous public owners | messages | none | - | - | - | - | - | 151 B |
| 3x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 186 B |
| 3x8 | confidential entry | messages | 3 | 3102 | 3241 | 4085 | 22 | 3 | 23 B |
| 3x8 | confidential entry | message bytes | 79 | 3113 | 3252 | 4096 | 22 | 3 | 1 B |
| 3x8 | anonymous entry | reply | none | - | - | - | - | - | 470 B |
| 3x8 | anonymous entry | messages | none | - | - | - | - | - | 470 B |
| 3x8 | anonymous entry | message bytes | none | - | - | - | - | - | 505 B |
| 4x2 | confidential signer | messages | 51 | 3072 | 3008 | 4088 | 23 | 3 | 26 B |
| 4x2 | confidential signer | message bytes | 1708 | 3080 | 3016 | 4096 | 23 | 3 | 1 B |
| 4x2 | confidential in-proof | messages | 51 | 3168 | 3105 | 4087 | 22 | 2 | 25 B |
| 4x2 | confidential in-proof | message bytes | 1709 | 3177 | 3114 | 4096 | 22 | 2 | 1 B |
| 4x2 | anonymous hidden | reply | 1870 | 3273 | 3241 | 4096 | 19 | 2 | 1 B |
| 4x2 | anonymous hidden | messages | 52 | 3269 | 3236 | 4092 | 19 | 2 | 30 B |
| 4x2 | anonymous hidden | message bytes | 1738 | 3273 | 3240 | 4096 | 19 | 2 | 1 B |
| 4x2 | anonymous public owners | reply | 1741 | 3080 | 3015 | 4096 | 23 | 3 | 1 B |
| 4x2 | anonymous public owners | messages | 48 | 3069 | 3003 | 4085 | 23 | 3 | 23 B |
| 4x2 | anonymous public owners | message bytes | 1609 | 3080 | 3014 | 4096 | 23 | 3 | 1 B |
| 4x2 | confidential entry | messages | 35 | 3080 | 3016 | 4096 | 23 | 3 | 34 B |
| 4x2 | confidential entry | message bytes | 1156 | 3080 | 3016 | 4096 | 23 | 3 | 1 B |
| 4x2 | anonymous entry | reply | 1286 | 3273 | 3209 | 4096 | 19 | 2 | 1 B |
| 4x2 | anonymous entry | messages | 34 | 3241 | 3176 | 4064 | 19 | 2 | 2 B |
| 4x2 | anonymous entry | message bytes | 1154 | 3273 | 3208 | 4096 | 19 | 2 | 1 B |
| 4x4 | confidential signer | messages | 38 | 3056 | 3060 | 4072 | 23 | 3 | 10 B |
| 4x4 | confidential signer | message bytes | 1282 | 3080 | 3084 | 4096 | 23 | 3 | 1 B |
| 4x4 | confidential in-proof | messages | 38 | 3152 | 3157 | 4071 | 22 | 2 | 9 B |
| 4x4 | confidential in-proof | message bytes | 1283 | 3177 | 3182 | 4096 | 22 | 2 | 1 B |
| 4x4 | anonymous hidden | reply | 481 | 3272 | 3308 | 4095 | 19 | 2 | 2 B |
| 4x4 | anonymous hidden | messages | 33 | 3243 | 3278 | 4066 | 19 | 2 | 4 B |
| 4x4 | anonymous hidden | message bytes | 1118 | 3273 | 3308 | 4096 | 19 | 2 | 1 B |
| 4x4 | anonymous public owners | reply | 438 | 3079 | 3082 | 4095 | 23 | 3 | 2 B |
| 4x4 | anonymous public owners | messages | 30 | 3077 | 3079 | 4093 | 23 | 3 | 31 B |
| 4x4 | anonymous public owners | message bytes | 989 | 3080 | 3082 | 4096 | 23 | 3 | 1 B |
| 4x4 | confidential entry | messages | 22 | 3064 | 3068 | 4080 | 23 | 3 | 18 B |
| 4x4 | confidential entry | message bytes | 730 | 3080 | 3084 | 4096 | 23 | 3 | 1 B |
| 4x4 | anonymous entry | reply | 286 | 3271 | 3275 | 4094 | 19 | 2 | 1 B |
| 4x4 | anonymous entry | messages | 16 | 3249 | 3252 | 4072 | 19 | 2 | 10 B |
| 4x4 | anonymous entry | message bytes | 534 | 3273 | 3276 | 4096 | 19 | 2 | 1 B |
| 4x4 | delegate | messages | 45 | 3246 | 3298 | 4066 | 19 | 2 | 4 B |
| 4x4 | delegate | message bytes | 1526 | 3276 | 3328 | 4096 | 19 | 2 | 1 B |
| 4x4 | anonymous delegate | reply | 519 | 3274 | 3325 | 4094 | 19 | 2 | 1 B |
| 4x4 | anonymous delegate | messages | 37 | 3267 | 3317 | 4087 | 19 | 2 | 25 B |
| 4x4 | anonymous delegate | message bytes | 1233 | 3276 | 3326 | 4096 | 19 | 2 | 1 B |
| 4x8 | confidential signer | messages | 13 | 3058 | 3198 | 4074 | 23 | 3 | 12 B |
| 4x8 | confidential signer | message bytes | 430 | 3080 | 3220 | 4096 | 23 | 3 | 1 B |
| 4x8 | confidential in-proof | messages | 13 | 3154 | 3295 | 4073 | 22 | 2 | 11 B |
| 4x8 | confidential in-proof | message bytes | 431 | 3177 | 3318 | 4096 | 22 | 2 | 1 B |
| 4x8 | anonymous hidden | reply | none | - | - | - | - | - | 87 B |
| 4x8 | anonymous hidden | messages | none | - | - | - | - | - | 87 B |
| 4x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 122 B |
| 4x8 | anonymous public owners | reply | none | - | - | - | - | - | 216 B |
| 4x8 | anonymous public owners | messages | none | - | - | - | - | - | 216 B |
| 4x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 251 B |
| 4x8 | confidential entry | messages | none | - | - | - | - | - | 87 B |
| 4x8 | confidential entry | message bytes | none | - | - | - | - | - | 122 B |
| 4x8 | anonymous entry | reply | none | - | - | - | - | - | 671 B |
| 4x8 | anonymous entry | messages | none | - | - | - | - | - | 671 B |
| 4x8 | anonymous entry | message bytes | none | - | - | - | - | - | 706 B |
| 5x2 | confidential signer | messages | 49 | 3036 | 2973 | 4085 | 24 | 3 | 23 B |
| 5x2 | confidential signer | message bytes | 1643 | 3047 | 2984 | 4096 | 24 | 3 | 1 B |
| 5x2 | confidential in-proof | messages | 49 | 3132 | 3070 | 4084 | 23 | 2 | 22 B |
| 5x2 | confidential in-proof | message bytes | 1644 | 3144 | 3082 | 4096 | 23 | 2 | 1 B |
| 5x2 | anonymous hidden | reply | 1805 | 3240 | 3209 | 4096 | 20 | 2 | 1 B |
| 5x2 | anonymous hidden | messages | 50 | 3233 | 3201 | 4089 | 20 | 2 | 27 B |
| 5x2 | anonymous hidden | message bytes | 1673 | 3240 | 3208 | 4096 | 20 | 2 | 1 B |
| 5x2 | anonymous public owners | reply | 1676 | 3047 | 2983 | 4096 | 24 | 3 | 1 B |
| 5x2 | anonymous public owners | messages | 46 | 3033 | 2968 | 4082 | 24 | 3 | 20 B |
| 5x2 | anonymous public owners | message bytes | 1544 | 3047 | 2982 | 4096 | 24 | 3 | 1 B |
| 5x2 | confidential entry | messages | 29 | 3044 | 2981 | 4093 | 24 | 3 | 31 B |
| 5x2 | confidential entry | message bytes | 955 | 3047 | 2984 | 4096 | 24 | 3 | 1 B |
| 5x2 | anonymous entry | reply | 1085 | 3240 | 3177 | 4096 | 20 | 2 | 1 B |
| 5x2 | anonymous entry | messages | 29 | 3239 | 3175 | 4095 | 20 | 2 | 33 B |
| 5x2 | anonymous entry | message bytes | 953 | 3240 | 3176 | 4096 | 20 | 2 | 1 B |
| 5x4 | confidential signer | messages | 36 | 3020 | 3025 | 4069 | 24 | 3 | 7 B |
| 5x4 | confidential signer | message bytes | 1217 | 3047 | 3052 | 4096 | 24 | 3 | 1 B |
| 5x4 | confidential in-proof | messages | 36 | 3116 | 3122 | 4068 | 23 | 2 | 6 B |
| 5x4 | confidential in-proof | message bytes | 1218 | 3144 | 3150 | 4096 | 23 | 2 | 1 B |
| 5x4 | anonymous hidden | reply | 459 | 3238 | 3275 | 4094 | 20 | 2 | 1 B |
| 5x4 | anonymous hidden | messages | 31 | 3207 | 3243 | 4063 | 20 | 2 | 1 B |
| 5x4 | anonymous hidden | message bytes | 1053 | 3240 | 3276 | 4096 | 20 | 2 | 1 B |
| 5x4 | anonymous public owners | reply | 416 | 3045 | 3049 | 4094 | 24 | 3 | 1 B |
| 5x4 | anonymous public owners | messages | 28 | 3041 | 3044 | 4090 | 24 | 3 | 28 B |
| 5x4 | anonymous public owners | message bytes | 924 | 3047 | 3050 | 4096 | 24 | 3 | 1 B |
| 5x4 | confidential entry | messages | 16 | 3028 | 3033 | 4077 | 24 | 3 | 15 B |
| 5x4 | confidential entry | message bytes | 529 | 3047 | 3052 | 4096 | 24 | 3 | 1 B |
| 5x4 | anonymous entry | reply | 219 | 3238 | 3243 | 4094 | 20 | 2 | 1 B |
| 5x4 | anonymous entry | messages | 10 | 3213 | 3217 | 4069 | 20 | 2 | 7 B |
| 5x4 | anonymous entry | message bytes | 333 | 3240 | 3244 | 4096 | 20 | 2 | 1 B |
| 4x16 | confidential signer | messages | none | - | - | - | - | - | 1240 B |
| 4x16 | confidential signer | message bytes | none | - | - | - | - | - | 1275 B |
| 4x16 | confidential in-proof | messages | none | - | - | - | - | - | 1239 B |
| 4x16 | confidential in-proof | message bytes | none | - | - | - | - | - | 1274 B |
| 4x16 | anonymous hidden | reply | none | - | - | - | - | - | 2568 B |
| 4x16 | anonymous hidden | messages | none | - | - | - | - | - | 2568 B |
| 4x16 | anonymous hidden | message bytes | none | - | - | - | - | - | 2603 B |
| 4x16 | anonymous public owners | reply | none | - | - | - | - | - | 2697 B |
| 4x16 | anonymous public owners | messages | none | - | - | - | - | - | 2697 B |
| 4x16 | anonymous public owners | message bytes | none | - | - | - | - | - | 2732 B |
| 5x8 | confidential signer | messages | 11 | 3022 | 3163 | 4071 | 24 | 3 | 9 B |
| 5x8 | confidential signer | message bytes | 365 | 3047 | 3188 | 4096 | 24 | 3 | 1 B |
| 5x8 | confidential in-proof | messages | 11 | 3118 | 3260 | 4070 | 23 | 2 | 8 B |
| 5x8 | confidential in-proof | message bytes | 366 | 3144 | 3286 | 4096 | 23 | 2 | 1 B |
| 5x8 | anonymous hidden | reply | none | - | - | - | - | - | 152 B |
| 5x8 | anonymous hidden | messages | none | - | - | - | - | - | 152 B |
| 5x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 187 B |
| 5x8 | anonymous public owners | reply | none | - | - | - | - | - | 281 B |
| 5x8 | anonymous public owners | messages | none | - | - | - | - | - | 281 B |
| 5x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 316 B |
| 5x8 | confidential entry | messages | none | - | - | - | - | - | 288 B |
| 5x8 | confidential entry | message bytes | none | - | - | - | - | - | 323 B |
| 5x8 | anonymous entry | reply | none | - | - | - | - | - | 872 B |
| 5x8 | anonymous entry | messages | none | - | - | - | - | - | 872 B |
| 5x8 | anonymous entry | message bytes | none | - | - | - | - | - | 907 B |
| 6x2 | confidential signer | messages | 47 | 3000 | 2938 | 4082 | 25 | 3 | 20 B |
| 6x2 | confidential signer | message bytes | 1578 | 3014 | 2952 | 4096 | 25 | 3 | 1 B |
| 6x2 | confidential in-proof | messages | 47 | 3096 | 3035 | 4081 | 24 | 2 | 19 B |
| 6x2 | confidential in-proof | message bytes | 1579 | 3111 | 3050 | 4096 | 24 | 2 | 1 B |
| 6x2 | anonymous hidden | reply | 1740 | 3207 | 3177 | 4096 | 21 | 2 | 1 B |
| 6x2 | anonymous hidden | messages | 48 | 3197 | 3166 | 4086 | 21 | 2 | 24 B |
| 6x2 | anonymous hidden | message bytes | 1608 | 3207 | 3176 | 4096 | 21 | 2 | 1 B |
| 6x2 | anonymous public owners | reply | 1611 | 3014 | 2951 | 4096 | 25 | 3 | 1 B |
| 6x2 | anonymous public owners | messages | 44 | 2997 | 2933 | 4079 | 25 | 3 | 17 B |
| 6x2 | anonymous public owners | message bytes | 1479 | 3014 | 2950 | 4096 | 25 | 3 | 1 B |
| 6x2 | confidential entry | messages | 23 | 3008 | 2946 | 4090 | 25 | 3 | 28 B |
| 6x2 | confidential entry | message bytes | 754 | 3014 | 2952 | 4096 | 25 | 3 | 1 B |
| 6x2 | anonymous entry | reply | 884 | 3207 | 3145 | 4096 | 21 | 2 | 1 B |
| 6x2 | anonymous entry | messages | 23 | 3203 | 3140 | 4092 | 21 | 2 | 30 B |
| 6x2 | anonymous entry | message bytes | 752 | 3207 | 3144 | 4096 | 21 | 2 | 1 B |
| 6x4 | confidential signer | messages | 34 | 2984 | 2990 | 4066 | 25 | 3 | 4 B |
| 6x4 | confidential signer | message bytes | 1152 | 3014 | 3020 | 4096 | 25 | 3 | 1 B |
| 6x4 | confidential in-proof | messages | 34 | 3080 | 3087 | 4065 | 24 | 2 | 3 B |
| 6x4 | confidential in-proof | message bytes | 1153 | 3111 | 3118 | 4096 | 24 | 2 | 1 B |
| 6x4 | anonymous hidden | reply | 438 | 3207 | 3245 | 4096 | 21 | 2 | 3 B |
| 6x4 | anonymous hidden | messages | 30 | 3205 | 3242 | 4094 | 21 | 2 | 32 B |
| 6x4 | anonymous hidden | message bytes | 988 | 3207 | 3244 | 4096 | 21 | 2 | 1 B |
| 6x4 | anonymous public owners | reply | 395 | 3014 | 3019 | 4096 | 25 | 3 | 3 B |
| 6x4 | anonymous public owners | messages | 26 | 3005 | 3009 | 4087 | 25 | 3 | 25 B |
| 6x4 | anonymous public owners | message bytes | 859 | 3014 | 3018 | 4096 | 25 | 3 | 1 B |
| 6x4 | confidential entry | messages | 10 | 2992 | 2998 | 4074 | 25 | 3 | 12 B |
| 6x4 | confidential entry | message bytes | 328 | 3014 | 3020 | 4096 | 25 | 3 | 1 B |
| 6x4 | anonymous entry | reply | 152 | 3205 | 3211 | 4094 | 21 | 2 | 1 B |
| 6x4 | anonymous entry | messages | 4 | 3177 | 3182 | 4066 | 21 | 2 | 4 B |
| 6x4 | anonymous entry | message bytes | 132 | 3207 | 3212 | 4096 | 21 | 2 | 1 B |
| 6x8 | confidential signer | messages | 9 | 2986 | 3128 | 4068 | 25 | 3 | 6 B |
| 6x8 | confidential signer | message bytes | 300 | 3014 | 3156 | 4096 | 25 | 3 | 1 B |
| 6x8 | confidential in-proof | messages | 9 | 3082 | 3225 | 4067 | 24 | 2 | 5 B |
| 6x8 | confidential in-proof | message bytes | 301 | 3111 | 3254 | 4096 | 24 | 2 | 1 B |
| 6x8 | anonymous hidden | reply | none | - | - | - | - | - | 217 B |
| 6x8 | anonymous hidden | messages | none | - | - | - | - | - | 217 B |
| 6x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 252 B |
| 6x8 | anonymous public owners | reply | none | - | - | - | - | - | 346 B |
| 6x8 | anonymous public owners | messages | none | - | - | - | - | - | 346 B |
| 6x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 381 B |
| 6x8 | confidential entry | messages | none | - | - | - | - | - | 489 B |
| 6x8 | confidential entry | message bytes | none | - | - | - | - | - | 524 B |
| 6x8 | anonymous entry | reply | none | - | - | - | - | - | 1073 B |
| 6x8 | anonymous entry | messages | none | - | - | - | - | - | 1073 B |
| 6x8 | anonymous entry | message bytes | none | - | - | - | - | - | 1108 B |
| 8x2 | confidential signer | messages | 43 | 2928 | 2868 | 4076 | 27 | 3 | 14 B |
| 8x2 | confidential signer | message bytes | 1448 | 2948 | 2888 | 4096 | 27 | 3 | 1 B |
| 8x2 | confidential in-proof | messages | 43 | 3024 | 2965 | 4075 | 26 | 2 | 13 B |
| 8x2 | confidential in-proof | message bytes | 1449 | 3045 | 2986 | 4096 | 26 | 2 | 1 B |
| 8x2 | anonymous hidden | reply | 1610 | 3141 | 3113 | 4096 | 23 | 2 | 1 B |
| 8x2 | anonymous hidden | messages | 44 | 3125 | 3096 | 4080 | 23 | 2 | 18 B |
| 8x2 | anonymous hidden | message bytes | 1478 | 3141 | 3112 | 4096 | 23 | 2 | 1 B |
| 8x2 | anonymous public owners | reply | 1481 | 2948 | 2887 | 4096 | 27 | 3 | 1 B |
| 8x2 | anonymous public owners | messages | 40 | 2925 | 2863 | 4073 | 27 | 3 | 11 B |
| 8x2 | anonymous public owners | message bytes | 1349 | 2948 | 2886 | 4096 | 27 | 3 | 1 B |
| 8x2 | confidential entry | messages | 11 | 2936 | 2876 | 4084 | 27 | 3 | 22 B |
| 8x2 | confidential entry | message bytes | 352 | 2948 | 2888 | 4096 | 27 | 3 | 1 B |
| 8x2 | anonymous entry | reply | 482 | 3141 | 3081 | 4096 | 23 | 2 | 1 B |
| 8x2 | anonymous entry | messages | 11 | 3131 | 3070 | 4086 | 23 | 2 | 24 B |
| 8x2 | anonymous entry | message bytes | 350 | 3141 | 3080 | 4096 | 23 | 2 | 1 B |
| 8x4 | confidential signer | messages | 31 | 2946 | 2954 | 4094 | 27 | 3 | 32 B |
| 8x4 | confidential signer | message bytes | 1022 | 2948 | 2956 | 4096 | 27 | 3 | 1 B |
| 8x4 | confidential in-proof | messages | 31 | 3042 | 3051 | 4093 | 26 | 2 | 31 B |
| 8x4 | confidential in-proof | message bytes | 1023 | 3045 | 3054 | 4096 | 26 | 2 | 1 B |
| 8x4 | anonymous hidden | reply | 394 | 3139 | 3179 | 4094 | 23 | 2 | 1 B |
| 8x4 | anonymous hidden | messages | 26 | 3133 | 3172 | 4088 | 23 | 2 | 26 B |
| 8x4 | anonymous hidden | message bytes | 858 | 3141 | 3180 | 4096 | 23 | 2 | 1 B |
| 8x4 | anonymous public owners | reply | 351 | 2946 | 2953 | 4094 | 27 | 3 | 1 B |
| 8x4 | anonymous public owners | messages | 22 | 2933 | 2939 | 4081 | 27 | 3 | 19 B |
| 8x4 | anonymous public owners | message bytes | 729 | 2948 | 2954 | 4096 | 27 | 3 | 1 B |
| 8x4 | confidential entry | messages | none | - | - | - | - | - | 39 B |
| 8x4 | confidential entry | message bytes | none | - | - | - | - | - | 74 B |
| 8x4 | anonymous entry | reply | none | - | - | - | - | - | 235 B |
| 8x4 | anonymous entry | messages | none | - | - | - | - | - | 235 B |
| 8x4 | anonymous entry | message bytes | none | - | - | - | - | - | 270 B |
| 8x8 | confidential signer | messages | 6 | 2948 | 3092 | 4096 | 27 | 3 | 34 B |
| 8x8 | confidential signer | message bytes | 170 | 2948 | 3092 | 4096 | 27 | 3 | 1 B |
| 8x8 | confidential in-proof | messages | 6 | 3044 | 3189 | 4095 | 26 | 2 | 33 B |
| 8x8 | confidential in-proof | message bytes | 171 | 3045 | 3190 | 4096 | 26 | 2 | 1 B |
| 8x8 | anonymous hidden | reply | none | - | - | - | - | - | 347 B |
| 8x8 | anonymous hidden | messages | none | - | - | - | - | - | 347 B |
| 8x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 382 B |
| 8x8 | anonymous public owners | reply | none | - | - | - | - | - | 476 B |
| 8x8 | anonymous public owners | messages | none | - | - | - | - | - | 476 B |
| 8x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 511 B |
| 8x8 | confidential entry | messages | none | - | - | - | - | - | 891 B |
| 8x8 | confidential entry | message bytes | none | - | - | - | - | - | 926 B |
| 8x8 | anonymous entry | reply | none | - | - | - | - | - | 1475 B |
| 8x8 | anonymous entry | messages | none | - | - | - | - | - | 1475 B |
| 8x8 | anonymous entry | message bytes | none | - | - | - | - | - | 1510 B |
| 12x2 | confidential signer | messages | 35 | 2785 | 2728 | 4065 | 31 | 3 | 3 B |
| 12x2 | confidential signer | message bytes | 1187 | 2816 | 2759 | 4096 | 31 | 3 | 1 B |
| 12x2 | confidential in-proof | messages | 35 | 2881 | 2825 | 4064 | 30 | 2 | 2 B |
| 12x2 | confidential in-proof | message bytes | 1188 | 2913 | 2857 | 4096 | 30 | 2 | 1 B |
| 12x2 | anonymous hidden | reply | 1349 | 3009 | 2984 | 4096 | 27 | 2 | 1 B |
| 12x2 | anonymous hidden | messages | 36 | 2982 | 2956 | 4069 | 27 | 2 | 7 B |
| 12x2 | anonymous hidden | message bytes | 1217 | 3009 | 2983 | 4096 | 27 | 2 | 1 B |
| 12x2 | anonymous public owners | reply | 1220 | 2816 | 2758 | 4096 | 31 | 3 | 1 B |
| 12x2 | anonymous public owners | messages | 33 | 2816 | 2757 | 4096 | 31 | 3 | 34 B |
| 12x2 | anonymous public owners | message bytes | 1088 | 2816 | 2757 | 4096 | 31 | 3 | 1 B |
| 12x4 | confidential signer | messages | 23 | 2803 | 2814 | 4083 | 31 | 3 | 21 B |
| 12x4 | confidential signer | message bytes | 761 | 2816 | 2827 | 4096 | 31 | 3 | 1 B |
| 12x4 | confidential in-proof | messages | 23 | 2899 | 2911 | 4082 | 30 | 2 | 20 B |
| 12x4 | confidential in-proof | message bytes | 762 | 2913 | 2925 | 4096 | 30 | 2 | 1 B |
| 12x4 | anonymous hidden | reply | 307 | 3007 | 3050 | 4094 | 27 | 2 | 1 B |
| 12x4 | anonymous hidden | messages | 18 | 2990 | 3032 | 4077 | 27 | 2 | 15 B |
| 12x4 | anonymous hidden | message bytes | 597 | 3009 | 3051 | 4096 | 27 | 2 | 1 B |
| 12x4 | anonymous public owners | reply | 264 | 2814 | 2824 | 4094 | 31 | 3 | 1 B |
| 12x4 | anonymous public owners | messages | 14 | 2790 | 2799 | 4070 | 31 | 3 | 8 B |
| 12x4 | anonymous public owners | message bytes | 468 | 2816 | 2825 | 4096 | 31 | 3 | 1 B |
| 12x8 | confidential signer | messages | none | - | - | - | - | - | 56 B |
| 12x8 | confidential signer | message bytes | none | - | - | - | - | - | 91 B |
| 12x8 | confidential in-proof | messages | none | - | - | - | - | - | 55 B |
| 12x8 | confidential in-proof | message bytes | none | - | - | - | - | - | 90 B |
| 12x8 | anonymous hidden | reply | none | - | - | - | - | - | 608 B |
| 12x8 | anonymous hidden | messages | none | - | - | - | - | - | 608 B |
| 12x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 643 B |
| 12x8 | anonymous public owners | reply | none | - | - | - | - | - | 737 B |
| 12x8 | anonymous public owners | messages | none | - | - | - | - | - | 737 B |
| 12x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 772 B |
| 16x2 | confidential signer | messages | 28 | 2675 | 2622 | 4087 | 35 | 3 | 25 B |
| 16x2 | confidential signer | message bytes | 927 | 2684 | 2631 | 4096 | 35 | 3 | 1 B |
| 16x2 | confidential in-proof | messages | 28 | 2771 | 2719 | 4086 | 34 | 2 | 24 B |
| 16x2 | confidential in-proof | message bytes | 928 | 2781 | 2729 | 4096 | 34 | 2 | 1 B |
| 16x2 | anonymous hidden | reply | 1089 | 2877 | 2856 | 4096 | 31 | 2 | 1 B |
| 16x2 | anonymous hidden | messages | 29 | 2872 | 2850 | 4091 | 31 | 2 | 29 B |
| 16x2 | anonymous hidden | message bytes | 957 | 2877 | 2855 | 4096 | 31 | 2 | 1 B |
| 16x2 | anonymous public owners | reply | 960 | 2684 | 2630 | 4096 | 35 | 3 | 1 B |
| 16x2 | anonymous public owners | messages | 25 | 2672 | 2617 | 4084 | 35 | 3 | 22 B |
| 16x2 | anonymous public owners | message bytes | 828 | 2684 | 2629 | 4096 | 35 | 3 | 1 B |
| 16x4 | confidential signer | messages | 15 | 2659 | 2674 | 4071 | 35 | 3 | 9 B |
| 16x4 | confidential signer | message bytes | 501 | 2684 | 2699 | 4096 | 35 | 3 | 1 B |
| 16x4 | confidential in-proof | messages | 15 | 2755 | 2771 | 4070 | 34 | 2 | 8 B |
| 16x4 | confidential in-proof | message bytes | 502 | 2781 | 2797 | 4096 | 34 | 2 | 1 B |
| 16x4 | anonymous hidden | reply | 221 | 2877 | 2924 | 4096 | 31 | 2 | 3 B |
| 16x4 | anonymous hidden | messages | 10 | 2846 | 2892 | 4065 | 31 | 2 | 3 B |
| 16x4 | anonymous hidden | message bytes | 337 | 2877 | 2923 | 4096 | 31 | 2 | 1 B |
| 16x4 | anonymous public owners | reply | 178 | 2684 | 2698 | 4096 | 35 | 3 | 3 B |
| 16x4 | anonymous public owners | messages | 7 | 2680 | 2693 | 4092 | 35 | 3 | 30 B |
| 16x4 | anonymous public owners | message bytes | 208 | 2684 | 2697 | 4096 | 35 | 3 | 1 B |
| 16x8 | confidential signer | messages | none | - | - | - | - | - | 316 B |
| 16x8 | confidential signer | message bytes | none | - | - | - | - | - | 351 B |
| 16x8 | confidential in-proof | messages | none | - | - | - | - | - | 315 B |
| 16x8 | confidential in-proof | message bytes | none | - | - | - | - | - | 350 B |
| 16x8 | anonymous hidden | reply | none | - | - | - | - | - | 868 B |
| 16x8 | anonymous hidden | messages | none | - | - | - | - | - | 868 B |
| 16x8 | anonymous hidden | message bytes | none | - | - | - | - | - | 903 B |
| 16x8 | anonymous public owners | reply | none | - | - | - | - | - | 997 B |
| 16x8 | anonymous public owners | messages | none | - | - | - | - | - | 997 B |
| 16x8 | anonymous public owners | message bytes | none | - | - | - | - | - | 1032 B |
| 24x2 | confidential signer | messages | 12 | 2388 | 2342 | 4064 | 43 | 3 | 2 B |
| 24x2 | confidential signer | message bytes | 406 | 2420 | 2374 | 4096 | 43 | 3 | 1 B |
| 24x2 | confidential in-proof | messages | 12 | 2484 | 2439 | 4063 | 42 | 2 | 1 B |
| 24x2 | confidential in-proof | message bytes | 407 | 2517 | 2472 | 4096 | 42 | 2 | 1 B |
| 24x2 | anonymous hidden | reply | 568 | 2613 | 2599 | 4096 | 39 | 2 | 1 B |
| 24x2 | anonymous hidden | messages | 13 | 2585 | 2570 | 4068 | 39 | 2 | 6 B |
| 24x2 | anonymous hidden | message bytes | 436 | 2613 | 2598 | 4096 | 39 | 2 | 1 B |
| 24x2 | anonymous public owners | reply | 439 | 2420 | 2373 | 4096 | 43 | 3 | 1 B |
| 24x2 | anonymous public owners | messages | 10 | 2419 | 2371 | 4095 | 43 | 3 | 33 B |
| 24x2 | anonymous public owners | message bytes | 307 | 2420 | 2372 | 4096 | 43 | 3 | 1 B |
| 24x4 | confidential signer | messages | 0 | 2405 | 2428 | 4081 | 43 | 3 | 20 B |
| 24x4 | confidential signer | message bytes | none | - | - | - | - | - | 20 B |
| 24x4 | confidential in-proof | messages | 0 | 2501 | 2525 | 4080 | 42 | 2 | 19 B |
| 24x4 | confidential in-proof | message bytes | none | - | - | - | - | - | 19 B |
| 24x4 | anonymous hidden | reply | none | - | - | - | - | - | 149 B |
| 24x4 | anonymous hidden | messages | none | - | - | - | - | - | 149 B |
| 24x4 | anonymous hidden | message bytes | none | - | - | - | - | - | 184 B |
| 24x4 | anonymous public owners | reply | none | - | - | - | - | - | 278 B |
| 24x4 | anonymous public owners | messages | none | - | - | - | - | - | 278 B |
| 24x4 | anonymous public owners | message bytes | none | - | - | - | - | - | 313 B |
| 32x2 | confidential signer | messages | none | - | - | - | - | - | 80 B |
| 32x2 | confidential signer | message bytes | none | - | - | - | - | - | 115 B |
| 32x2 | confidential in-proof | messages | none | - | - | - | - | - | 79 B |
| 32x2 | confidential in-proof | message bytes | none | - | - | - | - | - | 114 B |
| 32x2 | anonymous hidden | reply | none | - | - | - | - | - | 50 B |
| 32x2 | anonymous hidden | messages | none | - | - | - | - | - | 50 B |
| 32x2 | anonymous hidden | message bytes | none | - | - | - | - | - | 85 B |
| 32x2 | anonymous public owners | reply | none | - | - | - | - | - | 179 B |
| 32x2 | anonymous public owners | messages | none | - | - | - | - | - | 179 B |
| 32x2 | anonymous public owners | message bytes | none | - | - | - | - | - | 214 B |
| 40x2 | confidential signer | messages | none | - | - | - | - | - | 601 B |
| 40x2 | confidential signer | message bytes | none | - | - | - | - | - | 636 B |
| 40x2 | confidential in-proof | messages | none | - | - | - | - | - | 600 B |
| 40x2 | confidential in-proof | message bytes | none | - | - | - | - | - | 635 B |
| 40x2 | anonymous hidden | reply | none | - | - | - | - | - | 571 B |
| 40x2 | anonymous hidden | messages | none | - | - | - | - | - | 571 B |
| 40x2 | anonymous hidden | message bytes | none | - | - | - | - | - | 606 B |
| 40x2 | anonymous public owners | reply | none | - | - | - | - | - | 700 B |
| 40x2 | anonymous public owners | messages | none | - | - | - | - | - | 700 B |
| 40x2 | anonymous public owners | message bytes | none | - | - | - | - | - | 735 B |
