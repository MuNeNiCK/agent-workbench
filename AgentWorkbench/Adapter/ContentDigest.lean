import AgentWorkbench.Domain.ContentDigest

namespace AgentWorkbench.ContentDigest

partial def file (path : System.FilePath) : IO String := do
  IO.FS.withFile path IO.FS.Mode.read fun handle => do
    let rec loop (hasher : Blake3.C.Hasher) : IO String := do
      let chunk ← handle.read (1024 * 1024)
      if chunk.isEmpty then
        pure (encoded (hasher.finalizeWithLength 32).val)
      else
        loop (hasher.update chunk)
    loop (Blake3.C.Hasher.init ())

end AgentWorkbench.ContentDigest
