use std::path::PathBuf;

use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityCommand {
    Add(AuthorityAddArgs),
    Event {
        #[command(subcommand)]
        command: AuthorityEventCommand,
    },
    List(AuthorityListArgs),
    Provider {
        #[command(subcommand)]
        command: AuthorityProviderCommand,
    },
    Assertion {
        #[command(subcommand)]
        command: Box<AuthorityAssertionCommand>,
    },
    Grant {
        #[command(subcommand)]
        command: AuthorityGrantCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityProviderCommand {
    Verify(AuthorityProviderVerifyArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityProviderVerifyArgs {
    #[arg(long)]
    pub(crate) provider: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityAssertionCommand {
    Request {
        #[command(subcommand)]
        command: Box<AuthorityAssertionRequestCommand>,
    },
    Import(AuthorityAssertionImportArgs),
    Assemble(AuthorityAssertionAssembleArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AssertionRequestCommonArgs {
    #[arg(long)]
    pub(crate) provider: String,
    #[arg(long)]
    pub(crate) key_id: String,
    #[arg(long)]
    pub(crate) assertion_id: String,
    #[arg(long)]
    pub(crate) nonce: String,
    #[arg(long)]
    pub(crate) subject_kind: String,
    #[arg(long)]
    pub(crate) subject_digest: String,
    #[arg(long)]
    pub(crate) issued: String,
    #[arg(long)]
    pub(crate) expires: String,
    #[arg(long)]
    pub(crate) out: PathBuf,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityAssertionRequestCommand {
    RootGrant(AssertionRootGrantRequestArgs),
    CapabilityIssue(AssertionCapabilityIssueRequestArgs),
    GrantDelegate(AssertionGrantDelegateRequestArgs),
    GrantRevoke(AssertionGrantRevokeRequestArgs),
    ReviewProvenance(AssertionReviewProvenanceRequestArgs),
    LegacyReviewerBinding(AssertionLegacyReviewerBindingRequestArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AssertionRootGrantRequestArgs {
    #[command(flatten)]
    pub(crate) common: AssertionRequestCommonArgs,
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) maximum_target: String,
    #[arg(long)]
    pub(crate) allowed_roles: String,
    #[arg(long)]
    pub(crate) allowed_families: String,
    #[arg(long)]
    pub(crate) allowed_actions: String,
    #[arg(long)]
    pub(crate) maximum_depth: u64,
    #[arg(long)]
    pub(crate) expiry_ceiling: String,
}

#[derive(Debug, Args)]
pub(crate) struct AssertionCapabilityIssueRequestArgs {
    #[command(flatten)]
    pub(crate) common: AssertionRequestCommonArgs,
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) maximum_target: String,
    #[arg(long)]
    pub(crate) design_context: String,
    #[arg(long)]
    pub(crate) allowed_roles: String,
    #[arg(long)]
    pub(crate) allowed_families: String,
    #[arg(long)]
    pub(crate) allowed_actions: String,
    #[arg(long)]
    pub(crate) expiry_ceiling: String,
}

#[derive(Debug, Args)]
pub(crate) struct AssertionGrantDelegateRequestArgs {
    #[command(flatten)]
    pub(crate) common: AssertionRequestCommonArgs,
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) parent_grant: String,
    #[arg(long)]
    pub(crate) child_kind: String,
    #[arg(long)]
    pub(crate) child_digest: String,
    #[arg(long)]
    pub(crate) target: String,
    #[arg(long)]
    pub(crate) roles: String,
    #[arg(long)]
    pub(crate) families: String,
    #[arg(long)]
    pub(crate) actions: String,
    #[arg(long)]
    pub(crate) depth: u64,
    #[arg(long)]
    pub(crate) grant_expires: String,
}

#[derive(Debug, Args)]
pub(crate) struct AssertionGrantRevokeRequestArgs {
    #[command(flatten)]
    pub(crate) common: AssertionRequestCommonArgs,
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) target_grant: String,
    #[arg(long)]
    pub(crate) reason_digest: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct AssertionReviewProvenanceRequestArgs {
    #[command(flatten)]
    pub(crate) common: AssertionRequestCommonArgs,
    #[arg(long)]
    pub(crate) plan: String,
    #[arg(long)]
    pub(crate) target: String,
    #[arg(long)]
    pub(crate) provenance_kind: String,
    #[arg(long)]
    pub(crate) review_purpose: String,
    #[arg(long)]
    pub(crate) reference_digest: String,
}

#[derive(Debug, Args)]
pub(crate) struct AssertionLegacyReviewerBindingRequestArgs {
    #[command(flatten)]
    pub(crate) common: AssertionRequestCommonArgs,
    #[arg(long)]
    pub(crate) source_ledger_digest: String,
    #[arg(long)]
    pub(crate) source_generation: u64,
    #[arg(long)]
    pub(crate) source_reviewer_digest: String,
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityAssertionImportArgs {
    #[arg(long)]
    pub(crate) provider: String,
    #[arg(long)]
    pub(crate) purpose: String,
    #[arg(long)]
    pub(crate) file: PathBuf,
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityAssertionAssembleArgs {
    #[arg(long)]
    pub(crate) provider: String,
    #[arg(long)]
    pub(crate) request: PathBuf,
    #[arg(long)]
    pub(crate) signature: PathBuf,
    #[arg(long)]
    pub(crate) out: PathBuf,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityGrantCommand {
    RootIssue(AuthorityRootGrantArgs),
    Delegate(AuthorityGrantDelegateArgs),
    Revoke(AuthorityGrantRevokeArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityGrantDelegateArgs {
    pub(crate) parent_grant: String,
    #[arg(long)]
    pub(crate) grantor: String,
    #[arg(long)]
    pub(crate) grantee: String,
    #[arg(long)]
    pub(crate) grantor_assertion: String,
    #[arg(long)]
    pub(crate) target_scope: String,
    #[arg(long)]
    pub(crate) roles: String,
    #[arg(long)]
    pub(crate) decision_families: String,
    #[arg(long)]
    pub(crate) actions: String,
    #[arg(long)]
    pub(crate) delegation_depth: i64,
    #[arg(long)]
    pub(crate) expires: String,
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityRootGrantArgs {
    #[arg(long)]
    pub(crate) principal: String,
    #[arg(long)]
    pub(crate) assertion: String,
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) maximum_target: String,
    #[arg(long)]
    pub(crate) allowed_roles: String,
    #[arg(long)]
    pub(crate) allowed_families: String,
    #[arg(long)]
    pub(crate) allowed_actions: String,
    #[arg(long)]
    pub(crate) maximum_depth: i64,
    #[arg(long)]
    pub(crate) expires: String,
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityGrantRevokeArgs {
    #[arg(long)]
    pub(crate) grant: String,
    #[arg(long)]
    pub(crate) assertion: String,
}

#[derive(Debug, Subcommand)]
pub(crate) enum PrincipalCommand {
    Resolve(PrincipalResolveArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum OwnerCommand {
    Grant {
        #[command(subcommand)]
        command: OwnerGrantCommand,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum OwnerGrantCommand {
    RootIssue(OwnerGrantRootIssueArgs),
    Delegate(OwnerGrantDelegateArgs),
    Revoke(OwnerGrantRevokeArgs),
}

#[derive(Debug, Args)]
pub(crate) struct OwnerGrantRootIssueArgs {
    #[arg(long)]
    pub(crate) grantee: String,
    #[arg(long)]
    pub(crate) owner: String,
    #[arg(long)]
    pub(crate) target_scope: String,
    #[arg(long)]
    pub(crate) roles: String,
    #[arg(long)]
    pub(crate) decision_families: String,
    #[arg(long)]
    pub(crate) actions: String,
    #[arg(long)]
    pub(crate) delegation_depth: i64,
    #[arg(long)]
    pub(crate) expires: String,
    #[arg(long)]
    pub(crate) provider: String,
    #[arg(long)]
    pub(crate) human_assertion: String,
}

#[derive(Debug, Args)]
pub(crate) struct OwnerGrantDelegateArgs {
    pub(crate) parent_grant: String,
    #[arg(long)]
    pub(crate) grantee: String,
    #[arg(long)]
    pub(crate) target_scope: String,
    #[arg(long)]
    pub(crate) roles: String,
    #[arg(long)]
    pub(crate) decision_families: String,
    #[arg(long)]
    pub(crate) actions: String,
    #[arg(long)]
    pub(crate) delegation_depth: i64,
    #[arg(long)]
    pub(crate) expires: String,
    #[arg(long)]
    pub(crate) grantor: String,
    #[arg(long)]
    pub(crate) grantor_assertion: String,
}

#[derive(Debug, Args)]
pub(crate) struct OwnerGrantRevokeArgs {
    pub(crate) grant: String,
    #[arg(long)]
    pub(crate) grantor: String,
    #[arg(long)]
    pub(crate) grantor_assertion: String,
    #[arg(long)]
    pub(crate) reason: String,
    #[arg(long)]
    pub(crate) expected_current: String,
}

#[derive(Debug, Args)]
pub(crate) struct PrincipalResolveArgs {
    #[arg(long)]
    pub(crate) provider: String,
    #[arg(long)]
    pub(crate) assertion: String,
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityAddArgs {
    #[arg(long)]
    pub(crate) path: String,
    #[arg(long = "type")]
    pub(crate) authority_type: String,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) summary: Option<String>,
    #[arg(long, default_value_t = 90)]
    pub(crate) precedence: i64,
}

#[derive(Debug, Subcommand)]
pub(crate) enum AuthorityEventCommand {
    Add(AuthorityEventAddArgs),
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityEventAddArgs {
    #[arg(long = "type")]
    pub(crate) event_type: String,
    #[arg(long)]
    pub(crate) summary: String,
    #[arg(long)]
    pub(crate) scope: Option<String>,
    #[arg(long)]
    pub(crate) source: Option<String>,
    #[arg(long, default_value_t = 100)]
    pub(crate) precedence: i64,
}

#[derive(Debug, Args)]
pub(crate) struct AuthorityListArgs {
    #[arg(long)]
    pub(crate) scope: Option<String>,
}
