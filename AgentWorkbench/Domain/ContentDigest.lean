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

def encoded (digest : ByteArray) : String :=
  s!"blake3:{hex digest}"

def bytes (input : ByteArray) : String :=
  encoded (Blake3.C.hash input).val

def string (input : String) : String :=
  bytes input.toUTF8

end AgentWorkbench.ContentDigest
