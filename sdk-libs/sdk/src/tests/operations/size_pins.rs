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
    SizePin { n_in: 1, n_out: 2, bytes: [Some(1300), Some(1300), Some(1300), Some(1395), Some(1395), Some(1395), Some(1427), Some(1427), Some(1603), Some(1603), None] },
    SizePin { n_in: 1, n_out: 4, bytes: [Some(1654), Some(1654), Some(1654), Some(1749), Some(1749), Some(1749), Some(1781), Some(1781), Some(1957), Some(1957), None] },
    SizePin { n_in: 1, n_out: 8, bytes: [Some(2362), Some(2362), Some(2362), Some(2457), Some(2457), Some(2457), Some(2489), Some(2489), Some(2665), Some(2665), None] },
    SizePin { n_in: 2, n_out: 2, bytes: [Some(1365), Some(1365), Some(1365), Some(1460), Some(1460), Some(1460), Some(1492), Some(1492), Some(1804), Some(1804), Some(1324)] },
    SizePin { n_in: 2, n_out: 4, bytes: [Some(1719), Some(1719), Some(1719), Some(1814), Some(1814), Some(1814), Some(1846), Some(1846), Some(2158), Some(2158), None] },
    SizePin { n_in: 1, n_out: 16, bytes: [Some(3779), Some(3779), Some(3779), Some(3874), Some(3874), Some(3874), Some(3906), Some(3906), None, None, None] },
    SizePin { n_in: 2, n_out: 8, bytes: [Some(2427), Some(2427), Some(2427), Some(2522), Some(2522), Some(2522), Some(2554), Some(2554), Some(2866), Some(2866), None] },
    SizePin { n_in: 3, n_out: 2, bytes: [Some(1430), Some(1430), Some(1430), Some(1525), Some(1525), Some(1525), Some(1557), Some(1557), Some(2005), Some(2005), None] },
    SizePin { n_in: 3, n_out: 4, bytes: [Some(1784), Some(1784), Some(1784), Some(1879), Some(1879), Some(1879), Some(1911), Some(1911), Some(2359), Some(2359), None] },
    SizePin { n_in: 2, n_out: 16, bytes: [Some(3844), Some(3844), Some(3844), Some(3939), Some(3939), Some(3939), Some(3971), Some(3971), None, None, None] },
    SizePin { n_in: 3, n_out: 8, bytes: [Some(2492), Some(2492), Some(2492), Some(2587), Some(2587), Some(2587), Some(2619), Some(2619), Some(3067), Some(3067), None] },
    SizePin { n_in: 4, n_out: 2, bytes: [Some(1495), Some(1495), Some(1495), Some(1590), Some(1590), Some(1590), Some(1622), Some(1622), Some(2206), Some(2206), None] },
    SizePin { n_in: 4, n_out: 4, bytes: [Some(1849), Some(1849), Some(1849), Some(1944), Some(1944), Some(1944), Some(1976), Some(1976), Some(2560), Some(2560), Some(1808)] },
    SizePin { n_in: 4, n_out: 8, bytes: [Some(2557), Some(2557), Some(2557), Some(2652), Some(2652), Some(2652), Some(2684), Some(2684), Some(3268), Some(3268), None] },
    SizePin { n_in: 5, n_out: 2, bytes: [Some(1560), Some(1560), Some(1560), Some(1655), Some(1655), Some(1655), Some(1687), Some(1687), Some(2407), Some(2407), None] },
    SizePin { n_in: 5, n_out: 4, bytes: [Some(1914), Some(1914), Some(1914), Some(2009), Some(2009), Some(2009), Some(2041), Some(2041), Some(2761), Some(2761), None] },
    SizePin { n_in: 4, n_out: 16, bytes: [Some(3974), Some(3974), Some(3974), Some(4069), Some(4069), Some(4069), Some(4101), Some(4101), None, None, None] },
    SizePin { n_in: 5, n_out: 8, bytes: [Some(2622), Some(2622), Some(2622), Some(2717), Some(2717), Some(2717), Some(2749), Some(2749), Some(3469), Some(3469), None] },
    SizePin { n_in: 6, n_out: 2, bytes: [Some(1625), Some(1625), Some(1625), Some(1720), Some(1720), Some(1720), Some(1752), Some(1752), Some(2608), Some(2608), None] },
    SizePin { n_in: 6, n_out: 4, bytes: [Some(1979), Some(1979), Some(1979), Some(2074), Some(2074), Some(2074), Some(2106), Some(2106), Some(2962), Some(2962), None] },
    SizePin { n_in: 5, n_out: 16, bytes: [None, None, None, None, None, None, None, None, None, None, None] },
    SizePin { n_in: 6, n_out: 8, bytes: [Some(2687), Some(2687), Some(2687), Some(2782), Some(2782), Some(2782), Some(2814), Some(2814), Some(3670), Some(3670), None] },
    SizePin { n_in: 8, n_out: 2, bytes: [Some(1755), Some(1755), Some(1755), Some(1850), Some(1850), Some(1850), Some(1882), Some(1882), Some(3010), Some(3010), None] },
    SizePin { n_in: 8, n_out: 4, bytes: [Some(2109), Some(2109), Some(2109), Some(2204), Some(2204), Some(2204), Some(2236), Some(2236), Some(3364), Some(3364), None] },
    SizePin { n_in: 8, n_out: 8, bytes: [Some(2817), Some(2817), Some(2817), Some(2912), Some(2912), Some(2912), Some(2944), Some(2944), Some(4072), Some(4072), None] },
    SizePin { n_in: 8, n_out: 16, bytes: [None, None, None, None, None, None, None, None, None, None, None] },
    SizePin { n_in: 12, n_out: 2, bytes: [Some(2015), Some(2015), Some(2015), Some(2110), Some(2110), Some(2110), Some(2142), Some(2142), None, None, None] },
    SizePin { n_in: 12, n_out: 4, bytes: [Some(2369), Some(2369), Some(2369), Some(2464), Some(2464), Some(2464), Some(2496), Some(2496), None, None, None] },
    SizePin { n_in: 12, n_out: 8, bytes: [Some(3077), Some(3077), Some(3077), Some(3172), Some(3172), Some(3172), Some(3204), Some(3204), None, None, None] },
    SizePin { n_in: 16, n_out: 2, bytes: [Some(2275), Some(2275), Some(2275), Some(2370), Some(2370), Some(2370), Some(2402), Some(2402), None, None, None] },
    SizePin { n_in: 16, n_out: 4, bytes: [Some(2629), Some(2629), Some(2629), Some(2724), Some(2724), Some(2724), Some(2756), Some(2756), None, None, None] },
    SizePin { n_in: 16, n_out: 8, bytes: [Some(3337), Some(3337), Some(3337), Some(3432), Some(3432), Some(3432), Some(3464), Some(3464), None, None, None] },
    SizePin { n_in: 24, n_out: 2, bytes: [Some(2795), Some(2795), Some(2795), Some(2890), Some(2890), Some(2890), Some(2922), Some(2922), None, None, None] },
    SizePin { n_in: 24, n_out: 4, bytes: [Some(3149), Some(3149), Some(3149), Some(3244), Some(3244), Some(3244), Some(3276), Some(3276), None, None, None] },
    SizePin { n_in: 32, n_out: 2, bytes: [Some(3315), Some(3315), Some(3315), Some(3410), Some(3410), Some(3410), Some(3442), Some(3442), None, None, None] },
    SizePin { n_in: 40, n_out: 2, bytes: [Some(3835), Some(3835), Some(3835), Some(3930), Some(3930), Some(3930), Some(3962), Some(3962), None, None, None] },
    SizePin { n_in: 48, n_out: 2, bytes: [None, None, None, None, None, None, None, None, None, None, None] },
    SizePin { n_in: 49, n_out: 2, bytes: [None, None, None, None, None, None, None, None, None, None, None] },
];
