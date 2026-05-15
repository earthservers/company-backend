use company_ratelimits::ratelimiter::RatelimitResolver;
use rocket::{http::Method, Request};

pub struct DeltaRatelimits;

impl<'a> RatelimitResolver<Request<'a>> for DeltaRatelimits {
    fn resolve_bucket<'r>(&self, request: &'r Request<'_>) -> (&'r str, Option<&'r str>) {
        let (segment, resource, extra) = if request.routed_segment(0) == Some("0.8") {
            (
                request.routed_segment(1),
                request.routed_segment(2),
                request.routed_segment(3),
            )
        } else {
            (
                request.routed_segment(0),
                request.routed_segment(1),
                request.routed_segment(2),
            )
        };

        if let Some(segment) = segment {
            #[allow(clippy::redundant_locals)]
            let resource = resource;

            let method = request.method();
            match (segment, resource, method) {
                ("users", target, Method::Patch) => ("user_edit", target),
                ("users", _, _) => {
                    if let Some("default_avatar") = extra {
                        return ("default_avatar", None);
                    }

                    ("users", None)
                }
                ("bots", _, _) => ("bots", None),
                ("channels", Some(id), _) => {
                    if request.method() == Method::Post {
                        if let Some("messages") = extra {
                            return ("messaging", Some(id));
                        }
                        if let Some("interactions") = extra {
                            return ("interactions", Some(id));
                        }
                    }

                    ("channels", Some(id))
                }
                ("servers", Some(id), _) => ("servers", Some(id)),
                ("auth", _, _) => {
                    if request.method() == Method::Delete {
                        ("auth_delete", None)
                    } else {
                        ("auth", None)
                    }
                }
                // MC link OAuth flow — both /oauth/link-minecraft and
                // /oauth/link-minecraft/confirm. Tightly limited because
                // /confirm mints a JWT; an authenticated user spamming it
                // would generate unlimited valid tokens scoped to arbitrary
                // mojang_uuid intents (the cosmetics backend nonce dedup
                // blocks replay but doesn't bound mint volume).
                ("oauth", Some("link-minecraft"), _) => ("oauth_link", None),
                ("swagger", _, _) => ("swagger", None),
                ("safety", Some("report"), _) => ("safety_report", Some("report")),
                ("safety", _, _) => ("safety", None),
                _ => ("any", None),
            }
        } else {
            ("any", None)
        }
    }

    fn resolve_bucket_limit(&self, bucket: &str) -> u32 {
        match bucket {
            "user_edit" => 2,
            "users" => 20,
            "bots" => 10,
            "messaging" => 10,
            "interactions" => 5,
            "channels" => 15,
            "servers" => 5,
            "auth" => 15,
            "auth_delete" => 255,
            // 5 mints per window. Real users hit /confirm once per link;
            // legitimate retries (browser reload, mod restart) stay under
            // this. Anything higher is almost certainly automated.
            "oauth_link" => 5,
            "default_avatar" => 255,
            "swagger" => 100,
            "safety" => 15,
            "safety_report" => 3,
            _ => 20,
        }
    }
}
