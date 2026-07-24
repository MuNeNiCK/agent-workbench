mod authority;
mod commands;
mod design;
mod kpt;
mod repository;
mod review;
mod review_integrity;
mod update;
mod work;

// Later storage generations are installed only through the adjacent transition registry.
pub(crate) use update::{
    GENERATION_14_SQL, GENERATION_15_APPLICATION_LINK_SQL, GENERATION_15_SQL, GENERATION_16_SQL,
    GENERATION_17_SQL, GENERATION_18_SQL, GENERATION_19_SQL, GENERATION_20_SQL,
    GENERATION_21_FINDING_VERIFICATION_SQL, GENERATION_21_SQL, GENERATION_22_SQL,
    GENERATION_23_SQL, GENERATION_24_SQL,
};

pub(super) const SCHEMA_BATCHES: &[&str] = &[
    repository::SQL,
    work::SQL,
    commands::SQL,
    authority::SQL,
    design::SQL,
    review::SQL,
    review_integrity::SQL,
    kpt::SQL,
];
