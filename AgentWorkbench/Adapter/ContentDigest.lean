import Blake3.C

namespace AgentWorkbench.ContentDigest

private def hexDigit : Nat → Char
  | 0 => '0' | 1 => '1' | 2 => '2' | 3 => '3'
  | 4 => '4' | 5 => '5' | 6 => '6' | 7 => '7'
  | 8 => '8' | 9 => '9' | 10 => 'a' | 11 => 'b'
  | 12 => 'c' | 13 => 'd' | 14 => 'e' | _ => 'f'

private def hex (digest : ByteArray) : String :=
  String.ofList (digest.toList.flatMap (fun byte =>
    [hexDigit (byte.toNat / 16), hexDigit (byte.toNat % 16)]))

private def tagged (digest : ByteArray) : String :=
  s!"blake3:{hex digest}"

def bytes (input : ByteArray) : String :=
  tagged (Blake3.C.hash input).val

def string (input : String) : String :=
  bytes input.toUTF8

partial def file (path : System.FilePath) : IO String := do
  IO.FS.withFile path IO.FS.Mode.read fun handle => do
    let rec loop (hasher : Blake3.C.Hasher) : IO String := do
      let chunk ← handle.read (1024 * 1024)
      if chunk.isEmpty then
        pure (tagged (hasher.finalizeWithLength 32).val)
      else
        loop (hasher.update chunk)
    loop (Blake3.C.Hasher.init ())

end AgentWorkbench.ContentDigest
