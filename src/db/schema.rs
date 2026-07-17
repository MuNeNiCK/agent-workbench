mod authority;
mod commands;
mod design;
mod kpt;
mod repository;
mod review;
mod review_integrity;
mod work;

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
