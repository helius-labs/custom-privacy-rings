// One table, included by the SDK size test and the validator sweep.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Measured {
    Confidential,
    ConfidentialPolicy,
    ConfidentialEscrow,
    InProofOwner,
    InProofOwnerPolicy,
    InProofOwnerEscrow,
    Anonymous,
    AnonymousPolicy,
    AnonymousEntry,
    AnonymousEntryPolicy,
    Delegate,
}

impl Measured {
    const COLUMNS: [Self; 11] = [
        Self::Confidential,
        Self::ConfidentialPolicy,
        Self::ConfidentialEscrow,
        Self::InProofOwner,
        Self::InProofOwnerPolicy,
        Self::InProofOwnerEscrow,
        Self::Anonymous,
        Self::AnonymousPolicy,
        Self::AnonymousEntry,
        Self::AnonymousEntryPolicy,
        Self::Delegate,
    ];
}

#[derive(Debug, PartialEq, Eq)]
struct SizePin {
    n_in: usize,
    n_out: usize,
    /// One per [`Measured::COLUMNS`] entry, `None` for a shape the rail has no key for.
    bytes: [Option<usize>; Measured::COLUMNS.len()],
}

/// A value past `MAX_TRANSACTION_SIZE` fails assembly.
#[rustfmt::skip]
const SIZE_PINS: [SizePin; 38] = [
    SizePin { n_in: 1, n_out: 2, bytes: [Some(1300), Some(1300), Some(1300), Some(1395), Some(1395), Some(1395), Some(1363), Some(1363), Some(1539), Some(1539), None] },
    SizePin { n_in: 1, n_out: 4, bytes: [Some(1654), Some(1654), Some(1654), Some(1749), Some(1749), Some(1749), Some(1717), Some(1717), Some(1893), Some(1893), None] },
    SizePin { n_in: 1, n_out: 8, bytes: [Some(2362), Some(2362), Some(2362), Some(2457), Some(2457), Some(2457), Some(2425), Some(2425), Some(2601), Some(2601), None] },
    SizePin { n_in: 2, n_out: 2, bytes: [Some(1365), Some(1365), Some(1365), Some(1460), Some(1460), Some(1460), Some(1428), Some(1428), Some(1740), Some(1740), Some(1324)] },
    SizePin { n_in: 2, n_out: 4, bytes: [Some(1719), Some(1719), Some(1719), Some(1814), Some(1814), Some(1814), Some(1782), Some(1782), Some(2094), Some(2094), None] },
    SizePin { n_in: 1, n_out: 16, bytes: [Some(3779), Some(3779), Some(3779), Some(3874), Some(3874), Some(3874), Some(3842), Some(3842), None, None, None] },
    SizePin { n_in: 2, n_out: 8, bytes: [Some(2427), Some(2427), Some(2427), Some(2522), Some(2522), Some(2522), Some(2490), Some(2490), Some(2802), Some(2802), None] },
    SizePin { n_in: 3, n_out: 2, bytes: [Some(1430), Some(1430), Some(1430), Some(1525), Some(1525), Some(1525), Some(1493), Some(1493), Some(1941), Some(1941), None] },
    SizePin { n_in: 3, n_out: 4, bytes: [Some(1784), Some(1784), Some(1784), Some(1879), Some(1879), Some(1879), Some(1847), Some(1847), Some(2295), Some(2295), None] },
    SizePin { n_in: 2, n_out: 16, bytes: [Some(3844), Some(3844), Some(3844), Some(3939), Some(3939), Some(3939), Some(3907), Some(3907), None, None, None] },
    SizePin { n_in: 3, n_out: 8, bytes: [Some(2492), Some(2492), Some(2492), Some(2587), Some(2587), Some(2587), Some(2555), Some(2555), Some(3003), Some(3003), None] },
    SizePin { n_in: 4, n_out: 2, bytes: [Some(1495), Some(1495), Some(1495), Some(1590), Some(1590), Some(1590), Some(1558), Some(1558), Some(2142), Some(2142), None] },
    SizePin { n_in: 4, n_out: 4, bytes: [Some(1849), Some(1849), Some(1849), Some(1944), Some(1944), Some(1944), Some(1912), Some(1912), Some(2496), Some(2496), Some(1808)] },
    SizePin { n_in: 4, n_out: 8, bytes: [Some(2557), Some(2557), Some(2557), Some(2652), Some(2652), Some(2652), Some(2620), Some(2620), Some(3204), Some(3204), None] },
    SizePin { n_in: 5, n_out: 2, bytes: [Some(1560), Some(1560), Some(1560), Some(1655), Some(1655), Some(1655), Some(1623), Some(1623), Some(2343), Some(2343), None] },
    SizePin { n_in: 5, n_out: 4, bytes: [Some(1914), Some(1914), Some(1914), Some(2009), Some(2009), Some(2009), Some(1977), Some(1977), Some(2697), Some(2697), None] },
    SizePin { n_in: 4, n_out: 16, bytes: [Some(3974), Some(3974), Some(3974), Some(4069), Some(4069), Some(4069), Some(4037), Some(4037), None, None, None] },
    SizePin { n_in: 5, n_out: 8, bytes: [Some(2622), Some(2622), Some(2622), Some(2717), Some(2717), Some(2717), Some(2685), Some(2685), Some(3405), Some(3405), None] },
    SizePin { n_in: 6, n_out: 2, bytes: [Some(1625), Some(1625), Some(1625), Some(1720), Some(1720), Some(1720), Some(1688), Some(1688), Some(2544), Some(2544), None] },
    SizePin { n_in: 6, n_out: 4, bytes: [Some(1979), Some(1979), Some(1979), Some(2074), Some(2074), Some(2074), Some(2042), Some(2042), Some(2898), Some(2898), None] },
    SizePin { n_in: 5, n_out: 16, bytes: [None, None, None, None, None, None, None, None, None, None, None] },
    SizePin { n_in: 6, n_out: 8, bytes: [Some(2687), Some(2687), Some(2687), Some(2782), Some(2782), Some(2782), Some(2750), Some(2750), Some(3606), Some(3606), None] },
    SizePin { n_in: 8, n_out: 2, bytes: [Some(1755), Some(1755), Some(1755), Some(1850), Some(1850), Some(1850), Some(1818), Some(1818), Some(2946), Some(2946), None] },
    SizePin { n_in: 8, n_out: 4, bytes: [Some(2109), Some(2109), Some(2109), Some(2204), Some(2204), Some(2204), Some(2172), Some(2172), Some(3300), Some(3300), None] },
    SizePin { n_in: 8, n_out: 8, bytes: [Some(2817), Some(2817), Some(2817), Some(2912), Some(2912), Some(2912), Some(2880), Some(2880), Some(4008), Some(4008), None] },
    SizePin { n_in: 8, n_out: 16, bytes: [None, None, None, None, None, None, None, None, None, None, None] },
    SizePin { n_in: 12, n_out: 2, bytes: [Some(2015), Some(2015), Some(2015), Some(2110), Some(2110), Some(2110), Some(2078), Some(2078), None, None, None] },
    SizePin { n_in: 12, n_out: 4, bytes: [Some(2369), Some(2369), Some(2369), Some(2464), Some(2464), Some(2464), Some(2432), Some(2432), None, None, None] },
    SizePin { n_in: 12, n_out: 8, bytes: [Some(3077), Some(3077), Some(3077), Some(3172), Some(3172), Some(3172), Some(3140), Some(3140), None, None, None] },
    SizePin { n_in: 16, n_out: 2, bytes: [Some(2275), Some(2275), Some(2275), Some(2370), Some(2370), Some(2370), Some(2338), Some(2338), None, None, None] },
    SizePin { n_in: 16, n_out: 4, bytes: [Some(2629), Some(2629), Some(2629), Some(2724), Some(2724), Some(2724), Some(2692), Some(2692), None, None, None] },
    SizePin { n_in: 16, n_out: 8, bytes: [Some(3337), Some(3337), Some(3337), Some(3432), Some(3432), Some(3432), Some(3400), Some(3400), None, None, None] },
    SizePin { n_in: 24, n_out: 2, bytes: [Some(2795), Some(2795), Some(2795), Some(2890), Some(2890), Some(2890), Some(2858), Some(2858), None, None, None] },
    SizePin { n_in: 24, n_out: 4, bytes: [Some(3149), Some(3149), Some(3149), Some(3244), Some(3244), Some(3244), Some(3212), Some(3212), None, None, None] },
    SizePin { n_in: 32, n_out: 2, bytes: [Some(3315), Some(3315), Some(3315), Some(3410), Some(3410), Some(3410), Some(3378), Some(3378), None, None, None] },
    SizePin { n_in: 40, n_out: 2, bytes: [Some(3835), Some(3835), Some(3835), Some(3930), Some(3930), Some(3930), Some(3898), Some(3898), None, None, None] },
    SizePin { n_in: 48, n_out: 2, bytes: [None, None, None, None, None, None, None, None, None, None, None] },
    SizePin { n_in: 49, n_out: 2, bytes: [None, None, None, None, None, None, None, None, None, None, None] },
];
