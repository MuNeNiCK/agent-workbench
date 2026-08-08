import AgentWorkbench.Domain.Operation

namespace AgentWorkbenchTest.RouteReceipt

open AgentWorkbench

inductive Suite where
  | publicRoute
  | publicDesignWorkRoute
  | migratedPublicRoute
  deriving Repr, BEq, DecidableEq

structure Receipt where
  suite : Suite
  operation : Operation
  deriving Repr, BEq, DecidableEq

initialize receipts : IO.Ref (List Receipt) ← IO.mkRef []

/-- A receipt exists only after the linked public binary returned success for this operation. -/
def recordSuccessful (suite : Suite) (operation : Operation) : IO Unit := do
  if operation.kind == .mutation then
    receipts.modify (fun current => { suite, operation } :: current)

def recorded : IO (List Receipt) :=
  receipts.get

end AgentWorkbenchTest.RouteReceipt
