import MD4Lean
import AgentWorkbench.Adapter.ContentDigest
import AgentWorkbench.Domain.DesignSourceGraph

namespace AgentWorkbench.DesignMarkdown

private partial def textContent : MD4Lean.Text → String
  | .normal value | .br value | .softbr value | .entity value => value
  | .nullchar => "\u0000"
  | .em values | .strong values | .u values | .del values =>
      String.join (values.toList.map textContent)
  | .a _ _ _ values => String.join (values.toList.map textContent)
  | .img _ _ alt => String.join (alt.toList.map textContent)
  | .code values | .latexMath values | .latexMathDisplay values =>
      String.join values.toList
  | .wikiLink _ values => String.join (values.toList.map textContent)

private def texts (values : Array MD4Lean.Text) : String :=
  String.join (values.toList.map textContent)

private def unit
    (target path : String) (kind : DesignSourceUnitKind) (ancestry : List String)
    (text canonical : String) :
    DesignSourceUnit :=
  let digest := ContentDigest.string canonical
  { id := ContentDigest.string
      s!"{target}\n{path}\n{reprStr kind}\n{reprStr ancestry}\n{canonical}"
    target, path, kind, headingAncestry := ancestry, text, digest }

private abbrev HeadingStack := List (Nat × String)

private def ancestry (headings : HeadingStack) : List String := headings.map (·.2)

private def enterHeading (headings : HeadingStack) (level : Nat) (title : String) : HeadingStack :=
  headings.filter (fun heading => heading.1 < level) ++ [(level, title)]

private partial def blockSequence
    (target pathPrefix : String) (blocks : List MD4Lean.Block)
    (headings : HeadingStack) (index : Nat := 0) : List DesignSourceUnit × HeadingStack :=
  match blocks with
  | [] => ([], headings)
  | block :: rest =>
      let path := s!"{pathPrefix}/{index}"
      let canonical := reprStr block
      let (current, nextHeadings) := match block with
        | .p value =>
            ([unit target path .paragraph (ancestry headings) (texts value) canonical], headings)
        | .header level value =>
            let title := texts value
            ([unit target path .heading (ancestry headings) title canonical],
              enterHeading headings level title)
        | .code _ _ _ lines =>
            ([unit target path .code (ancestry headings)
              (String.intercalate "\n" lines.toList) canonical], headings)
        | .html lines =>
            ([unit target path .html (ancestry headings)
              (String.intercalate "\n" lines.toList) canonical], headings)
        | .table _ _ =>
            ([unit target path .table (ancestry headings) canonical canonical], headings)
        | .blockquote children =>
            let nested := blockSequence target s!"{path}/quote" children.toList headings
            (nested.1, headings)
        | .ul _ _ items | .ol _ _ _ items =>
            let nested := items.toList.zipIdx.flatMap fun (item, itemIndex) =>
              let itemPath := s!"{path}/item/{itemIndex}"
              let itemCanonical := reprStr item
              let descendantItems := item.contents.toList.zipIdx.flatMap fun (child, childIndex) =>
                match child with
                | .ul _ _ _ | .ol _ _ _ _ =>
                    (blockSequence target s!"{itemPath}/nested/{childIndex}" [child] headings).1
                | _ => []
              unit target itemPath .listItem (ancestry headings) itemCanonical itemCanonical ::
                descendantItems
            (nested, headings)
        | .hr => ([], headings)
      let following := blockSequence target pathPrefix rest nextHeadings (index + 1)
      (current ++ following.1, following.2)

def inspect (target source : String) : Except String (List DesignSourceUnit) := do
  let document ← match MD4Lean.parse source MD4Lean.MD_DIALECT_COMMONMARK with
    | some value => pure value
    | none => throw s!"Markdown parser rejected {target}"
  pure (blockSequence target "block" document.blocks.toList []).1

end AgentWorkbench.DesignMarkdown
