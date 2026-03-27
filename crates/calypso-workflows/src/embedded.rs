use crate::{GhaWorkflowRaw, Workflow};

const CALYPSO_DEFAULT_DEPLOYMENT_WORKFLOW: &str = include_str!(
    "../../../calypso-blueprint/examples/workflows/calypso-default-deployment-workflow.yaml"
);
const CALYPSO_DEFAULT_FEATURE_WORKFLOW: &str = include_str!(
    "../../../calypso-blueprint/examples/workflows/calypso-default-feature-workflow.yaml"
);
const CALYPSO_DEPLOYMENT_REQUEST: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-deployment-request.yaml");
const CALYPSO_FEATURE_REQUEST: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-feature-request.yaml");
const CALYPSO_IMPLEMENTATION_LOOP: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-implementation-loop.yaml");
const CALYPSO_ORCHESTRATOR_STARTUP: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-orchestrator-startup.yaml");
const CALYPSO_PLANNING: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-planning.yaml");
const CALYPSO_PR_REVIEW_MERGE: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-pr-review-merge.yaml");
const CALYPSO_RELEASE_REQUEST: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-release-request.yaml");
const CALYPSO_SAVE_STATE: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-save-state.yaml");

/// Static registry of all embedded `calypso-*.yaml` workflow files.
pub struct EmbeddedWorkflowLibrary;

impl EmbeddedWorkflowLibrary {
    /// Returns all embedded workflows as `(filename_stem, raw_yaml)` pairs.
    pub fn list() -> &'static [(&'static str, &'static str)] {
        &[
            (
                "calypso-default-deployment-workflow",
                CALYPSO_DEFAULT_DEPLOYMENT_WORKFLOW,
            ),
            (
                "calypso-default-feature-workflow",
                CALYPSO_DEFAULT_FEATURE_WORKFLOW,
            ),
            ("calypso-deployment-request", CALYPSO_DEPLOYMENT_REQUEST),
            ("calypso-feature-request", CALYPSO_FEATURE_REQUEST),
            ("calypso-implementation-loop", CALYPSO_IMPLEMENTATION_LOOP),
            ("calypso-orchestrator-startup", CALYPSO_ORCHESTRATOR_STARTUP),
            ("calypso-planning", CALYPSO_PLANNING),
            ("calypso-pr-review-merge", CALYPSO_PR_REVIEW_MERGE),
            ("calypso-release-request", CALYPSO_RELEASE_REQUEST),
            ("calypso-save-state", CALYPSO_SAVE_STATE),
        ]
    }

    /// Look up a workflow by its filename stem (e.g. `"calypso-planning"`).
    pub fn get(name: &str) -> Option<&'static str> {
        Self::list()
            .iter()
            .find(|(stem, _)| *stem == name)
            .map(|(_, yaml)| *yaml)
    }

    /// Parse a raw GHA YAML string into a [`Workflow`].
    ///
    /// This parses the GitHub Actions format and derives the state machine
    /// representation (states, transitions, kinds) from the GHA structure.
    pub fn parse(yaml: &str) -> Result<Workflow, serde_yaml::Error> {
        let raw: GhaWorkflowRaw = serde_yaml::from_str(yaml)?;
        Ok(Workflow::from_gha(raw))
    }
}
