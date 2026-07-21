use crate::review::{
    decode_opaque_component, encode_opaque_component, encode_opaque_task_ref,
    parse_correction_tokens,
};

#[test]
fn repository_surfaces_are_never_source_correction_authority() {
    for repository_surface in [
        "repository:create:src/new.rs",
        "repository:edit:src/cli/design_flow.rs",
        "repository:delete:tests/cli/compatibility.rs",
        "repository:edit:.git/config",
        "repository:edit:.agent-workbench/state.db",
        "repository:edit:target/debug/agent-workbench",
        "repository:edit:../outside.rs",
    ] {
        let error = parse_correction_tokens(repository_surface).err().unwrap();
        assert!(
            error
                .to_string()
                .contains("not source-correction authority"),
            "{repository_surface}: {error:#}"
        );
    }

    let markdown = parse_correction_tokens(
        "design:edit:01-context.md,plan:create:plans/fix.md,docs:edit:README.md,workflow:edit:skills/agent-workbench/SKILL.md",
    )
    .unwrap();
    assert_eq!(markdown.len(), 4);
}

#[test]
fn opaque_correction_components_are_lossless_canonical_and_vocabulary_free() {
    let key = "要件/α: punctuation !? 🚀";
    let component = encode_opaque_component(key);
    assert_eq!(
        decode_opaque_component(&component, "test key").unwrap(),
        key
    );

    let task = encode_opaque_task_ref(key);
    let surfaces = format!(
        "transition:design-decompose:7/11,transition:task-accept-out-of-scope:{task},transition:phase-create:11/7/@implementation/implementation/1/implementation,transition:phase-assign:@implementation/{task}"
    );
    let parsed = parse_correction_tokens(&surfaces).unwrap();
    assert_eq!(parsed[1].target, task);
    assert_eq!(parsed[3].target, format!("@implementation/{task}"));

    for invalid in [
        "transition:design-decompose:7/11,transition:task-accept-out-of-scope:@task/REQ-001",
        "transition:design-decompose:7/11,transition:task-accept-out-of-scope:@task/b64:YQ==",
        "transition:design-decompose:7/11,transition:task-accept-out-of-scope:@task/b64:YR",
        "transition:design-decompose:7/11,transition:task-accept-out-of-scope:@task/b64:_w",
    ] {
        assert!(parse_correction_tokens(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn decomposition_reconciliation_target_names_the_exact_opaque_project_path() {
    let path = ".agent-workbench/designs/package/plans/後継 plan,1.md";
    let target = format!("7/11/{}", encode_opaque_component(path));
    let parsed =
        parse_correction_tokens(&format!("transition:decomposition-plan-reconcile:{target}"))
            .unwrap();
    assert_eq!(parsed[0].target, target);

    for invalid in [
        "transition:decomposition-plan-reconcile:7/11",
        "transition:decomposition-plan-reconcile:7/11/b64:YQ==",
        "transition:decomposition-plan-reconcile:7/11/b64:Li4veC5tZA",
    ] {
        assert!(parse_correction_tokens(invalid).is_err(), "{invalid}");
    }
}
