// Migration: switch decorations from price_cents to price_coins.
//
// Run on the MongoDB instance:
//   mongosh "mongodb://..." < migrations/20260516_add_price_coins_to_decorations.js
//
// With Nexus as the canonical coin ledger, decorations price in
// EarthCoins (1 coin = $0.01 USD at the current peg). There is no
// legacy production data to preserve, so we both backfill
// `price_coins` from any pre-existing `price_cents` *and* drop the
// old field in the same pass. Same for the purchase record's
// `price_paid_cents` → `price_paid_coins` rename.

print("=== Switch decorations to price_coins ===");
print("Starting migration at " + new Date().toISOString());

// 1. Decorations
if (db.getCollectionNames().includes("decorations")) {
    const totalBefore = db.decorations.countDocuments({});
    const missing = db.decorations.countDocuments({ price_coins: { $exists: false } });
    print(`decorations: ${totalBefore} total (missing price_coins: ${missing})`);

    if (missing > 0) {
        const res = db.decorations.updateMany(
            { price_coins: { $exists: false } },
            // Aggregation-pipeline update so $set can reference another field.
            [{ $set: { price_coins: { $ifNull: ["$price_cents", 0] } } }]
        );
        print(`  backfilled price_coins on ${res.modifiedCount} decorations`);
    }

    const unsetRes = db.decorations.updateMany(
        { price_cents: { $exists: true } },
        { $unset: { price_cents: "" } }
    );
    print(`  dropped price_cents from ${unsetRes.modifiedCount} decorations`);
} else {
    print("decorations collection does not exist yet — skipped.");
}

// 2. Decoration purchases (price_paid_cents → price_paid_coins)
if (db.getCollectionNames().includes("decoration_purchases")) {
    const totalP = db.decoration_purchases.countDocuments({});
    print(`decoration_purchases: ${totalP} total`);

    const renameRes = db.decoration_purchases.updateMany(
        { price_paid_cents: { $exists: true } },
        { $rename: { price_paid_cents: "price_paid_coins" } }
    );
    print(`  renamed price_paid_cents → price_paid_coins on ${renameRes.modifiedCount} purchases`);
} else {
    print("decoration_purchases collection does not exist yet — skipped.");
}

// 3. Drop the dead creator-earnings collections.
// Coin payouts now live in Earth Nexus; the Company-side
// `creator_earnings`, `creator_balances`, and `cashout_requests`
// collections are orphaned by the purchase.rs rewrite. Drop them so
// nothing reads stale data.
const deadCollections = ["creator_earnings", "creator_balances", "cashout_requests"];
const existing = db.getCollectionNames();
for (const name of deadCollections) {
    if (existing.includes(name)) {
        db.getCollection(name).drop();
        print(`  dropped collection ${name}`);
    }
}

print("\n=== Migration complete ===");
