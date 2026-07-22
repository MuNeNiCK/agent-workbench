import AgentWorkbench.Domain.Identity
import AgentWorkbench.Domain.Facts

namespace AgentWorkbench.Domain.Design

open AgentWorkbench.Domain

structure DesignVersion where
  id : DesignId
  revision : Revision
  approved : Bool
deriving DecidableEq, Repr

structure Requirement where
  key : String
  active : Bool
deriving DecidableEq, Repr

end AgentWorkbench.Domain.Design
