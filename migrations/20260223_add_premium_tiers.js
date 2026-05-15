// Migration: Expand subscription tiers from {Free, Premium} to {Free, Basic, Pro, Ultra}
//
// Run this on the MongoDB instance:
//   mongosh "mongodb://..." < migrations/20260223_add_premium_tiers.js
//
// This migration:
// 1. Renames existing "Premium" subscriptions to "Pro" (closest match)
// 2. Creates indexes for the clips collection (for future use)
// 3. Adds an index on stripe_customer_id for lookup efficiency

print("=== Premium Tiers Migration ===");
print("Starting migration at " + new Date().toISOString());

// 1. Migrate existing "Premium" subscriptions to "Pro"
// The serde(alias = "Premium") on Pro handles deserialization, but let's also
// update the stored values for consistency.
const premiumCount = db.users.countDocuments({
    "subscription.tier": "Premium"
});

print(`Found ${premiumCount} users with "Premium" tier`);

if (premiumCount > 0) {
    const result = db.users.updateMany(
        { "subscription.tier": "Premium" },
        { $set: { "subscription.tier": "Pro" } }
    );
    print(`Updated ${result.modifiedCount} users from "Premium" to "Pro"`);
}

// 2. Create clips collection and indexes (for future use)
// Collection will be auto-created on first insert, but indexes should be ready
if (!db.getCollectionNames().includes("clips")) {
    db.createCollection("clips");
    print("Created 'clips' collection");
}

db.clips.createIndex({ creator_id: 1 }, { name: "idx_clips_creator" });
db.clips.createIndex({ streamer_id: 1 }, { name: "idx_clips_streamer" });
db.clips.createIndex({ stream_session_id: 1 }, { name: "idx_clips_session" });
db.clips.createIndex({ channel_id: 1 }, { name: "idx_clips_channel" });
db.clips.createIndex({ server_id: 1 }, { name: "idx_clips_server", sparse: true });
db.clips.createIndex({ created_at: -1 }, { name: "idx_clips_created" });
db.clips.createIndex({ retention_until: 1 }, {
    name: "idx_clips_retention",
    sparse: true
    // Note: Use this index with a cron job to clean up expired clips
});
print("Created clip indexes");

// 3. Add index on stripe_customer_id for efficient lookups
db.users.createIndex(
    { "subscription.stripe_customer_id": 1 },
    {
        name: "idx_users_stripe_customer",
        sparse: true,
        unique: true
    }
);
print("Created stripe_customer_id index");

// 4. Add index on subscription tier for analytics queries
db.users.createIndex(
    { "subscription.tier": 1 },
    { name: "idx_users_subscription_tier", sparse: true }
);
print("Created subscription tier index");

// 5. Verification
const tierCounts = db.users.aggregate([
    { $group: {
        _id: "$subscription.tier",
        count: { $sum: 1 }
    }},
    { $sort: { _id: 1 } }
]).toArray();

print("\nSubscription tier distribution:");
tierCounts.forEach(t => {
    print(`  ${t._id || "null (Free)"}: ${t.count} users`);
});

print("\n=== Migration complete ===");
