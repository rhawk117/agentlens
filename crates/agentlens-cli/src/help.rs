pub(crate) const ADDRESSES: &str = "\
addresses

  file.py#Symbol            a function or class
  file.py#Class.method      a method; nest with dots
  file.py#__module__        imports and module-level code
  file.py#                  the outline, no bodies
  file.py#L10-L20           a line span (escape hatch, not the idiom)

also accepted, rewritten for you

  file.py                   -> file.py#
  file.py 10 20             -> file.py#L10-L20
  file.py:10-20             -> file.py#L10-L20
  file.py::Symbol           -> file.py#Symbol
  Symbol                    -> the file that defines it

  rewriting happens only when the file exists. a bare name is looked up
  in the index; if it is ambiguous you get the candidate addresses.

symbols are re-resolved every call, so an address survives edits that
line numbers do not.
";

pub(crate) const GRAMMAR: &str = "\
an address is a file and something in it: `file.py#Class.method`.
`file.py#` is the outline. run `agentlens help addresses` for the rest.";

pub(crate) fn topic(name: &str) -> Option<&'static str> {
    match name {
        "addresses" | "address" => Some(ADDRESSES),
        _ => None,
    }
}

pub(crate) const TOPICS: &str = "addresses";
