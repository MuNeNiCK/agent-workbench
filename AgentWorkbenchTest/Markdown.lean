import AgentWorkbenchTest.Fixture
import AgentWorkbench.Adapter.DesignMarkdown

namespace AgentWorkbenchTest.Markdown

open AgentWorkbench AgentWorkbenchTest

private def requireUnit
    (units : List DesignSourceUnit) (kind : DesignSourceUnitKind) (text : String) : IO DesignSourceUnit :=
  match units.find? (fun sourceUnit => sourceUnit.kind == kind && sourceUnit.text == text) with
  | some value => pure value
  | none => throw (IO.userError s!"canonical Markdown graph omitted {reprStr kind}: {text}")

def run : IO Unit := do
  let units ← fromExcept <| AgentWorkbench.DesignMarkdown.inspect
    "file:.agent-workbench/design/product/example.md"
    "# Product\n\nTop requirement.\n\n## Storage\n\nNested requirement.\n\n- first item\n- second item\n"
  let product ← requireUnit units .heading "Product"
  let top ← requireUnit units .paragraph "Top requirement."
  let storage ← requireUnit units .heading "Storage"
  let nested ← requireUnit units .paragraph "Nested requirement."
  expect (product.headingAncestry.isEmpty)
    "top-level heading acquired a nonexistent ancestor"
  expect (top.headingAncestry == ["Product"] && storage.headingAncestry == ["Product"] &&
    nested.headingAncestry == ["Product", "Storage"])
    "canonical Markdown units did not retain their exact heading ancestry"
  expect (units.any (fun sourceUnit => sourceUnit.kind == .listItem &&
    sourceUnit.headingAncestry == ["Product", "Storage"]))
    "nested list-item content was omitted or detached from its heading ancestry"
  expect ((units.filter (·.kind == .listItem)).length == 2 &&
    !(units.any fun sourceUnit => sourceUnit.kind == .paragraph &&
      (sourceUnit.text == "first item" || sourceUnit.text == "second item")))
    "list-item content was duplicated as both wrapper and child paragraph units"
  let quoted ← fromExcept <| AgentWorkbench.DesignMarkdown.inspect
    "file:.agent-workbench/design/product/quote.md" "> Quoted requirement.\n"
  expect (quoted.length == 1 && quoted.head?.any (·.kind == .paragraph))
    "blockquote wrapper leaked as a second content-bearing unit"
  let identities := units.map (·.id)
  expect (identities.all fun identity => identities.count identity == 1)
    "canonical Markdown graph produced duplicate unit identities"
  let unclassified := { design with sourceUnitDispositions := [] }
  expectError (validateState { baseState with designRevisions := [unclassified] })
    "Design accepted an unclassified content-bearing Markdown unit"
  let missingChoices := { design with statementCoverage := [{
    statementId := statement.id, sourceUnitIds := [sourceUnit.id]
    leanClaims := {}, acceptanceCriteria := {}, implementationRequired := true }] }
  expectError (validateState { baseState with designRevisions := [missingChoices] })
    "Design accepted a Statement without explicit Claim and Acceptance choices"
  let unselectedCriterion := { design with statementCoverage := [{
    statementId := statement.id, sourceUnitIds := [sourceUnit.id]
    leanClaims := { noSelectionReason := some "no logical Claim is needed" }
    acceptanceCriteria := { noSelectionReason := some "no Criterion is selected" }
    implementationRequired := true }] }
  expectError (validateState { baseState with designRevisions := [unselectedCriterion] })
    "Design accepted a declared Criterion that no Statement selected"
  let unreasoned := { design with sourceUnitDispositions := [{
    unitId := sourceUnit.id, role := .rationale }] }
  expectError (validateState { baseState with designRevisions := [unreasoned] })
    "Design accepted non-authoritative rationale without an explicit reason"
  let duplicated := { design with sourceUnitDispositions := [
    { unitId := sourceUnit.id, role := .requirement },
    { unitId := sourceUnit.id, role := .requirement }] }
  expectError (validateState { baseState with designRevisions := [duplicated] })
    "Design accepted duplicate classification for one Markdown unit"
  let stale := { design with sourceUnitDispositions := [{
    unitId := "unit-from-another-capture", role := .requirement }] }
  expectError (validateState { baseState with designRevisions := [stale] })
    "Design accepted a stale or unknown Markdown unit identity"
  let crossDocumentUnit := { sourceUnit with
    target := "file:.agent-workbench/design/product/unarchived.md" }
  let crossDocument := { design with
    sourceUnits := [crossDocumentUnit]
    sourceUnitDispositions := [{ unitId := crossDocumentUnit.id, role := .requirement }] }
  expectError (validateState { baseState with designRevisions := [crossDocument] })
    "Design accepted a source unit from outside its archived source manifest"
  let rationaleAuthority := { design with
    sourceUnitDispositions := [{
      unitId := sourceUnit.id, role := .rationale, reason := some "background only" }] }
  expectError (validateState { baseState with designRevisions := [rationaleAuthority] })
    "Design allowed rationale to ground normative Statement authority"

end AgentWorkbenchTest.Markdown
