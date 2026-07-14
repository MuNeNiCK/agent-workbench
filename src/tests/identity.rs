use crate::identity::*;

#[test]
fn canonical_objects_sort_keys_and_normalize_documents() {
    let value = CanonicalValue::object([
        (
            "z",
            CanonicalValue::string(normalize_document("e\u{301}\r\n")),
        ),
        ("a", CanonicalValue::Null),
    ]);
    assert_eq!(
        String::from_utf8(canonical_bytes(&value)).unwrap(),
        "{\"a\":null,\"z\":\"é\\n\"}"
    );
    assert_eq!(
        domain_digest(b"AWB-GOLDEN-v1\0", &value),
        "8cfa52faa266f79e961e0003353999b0e3c58b6534663d5767832d81ddba3b7d"
    );
}

#[test]
fn typed_handles_reject_other_domains_and_noncanonical_hex() {
    let binding = CanonicalValue::object([("owner", CanonicalValue::string("1"))]);
    let plan = PlanHandle::derive(b"AWB-PLAN-HANDLE-v1\0", &binding);
    assert!(PlanHandle::parse(plan.as_str()).is_ok());
    assert!(OwnerHandle::parse(plan.as_str()).is_err());
    assert!(PlanHandle::parse(&format!("plan_{}", "A".repeat(64))).is_err());
}
