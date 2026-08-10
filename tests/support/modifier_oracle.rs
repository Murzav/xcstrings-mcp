pub const VALID_CASES: [&str; 72] = [
    "%d", "%D", "%i", "%o", "%O", "%u", "%U", "%x", "%X", "%e", "%E", "%f", "%F", "%g", "%G", "%a",
    "%A", "%c", "%C", "%s", "%S", "%p", "%n", "%@", "%hd", "%ho", "%hu", "%hx", "%hX", "%hhd",
    "%hho", "%hhu", "%hhx", "%hhX", "%ld", "%lo", "%lu", "%lx", "%lX", "%lld", "%llo", "%llu",
    "%llx", "%llX", "%qd", "%qo", "%qu", "%qx", "%qX", "%zd", "%zo", "%zu", "%zx", "%zX", "%td",
    "%to", "%tu", "%tx", "%tX", "%jd", "%jo", "%ju", "%jx", "%jX", "%La", "%LA", "%Le", "%LE",
    "%Lf", "%LF", "%Lg", "%LG",
];

pub const INVALID_CASES: [&str; 168] = [
    "%hD", "%hi", "%hO", "%hU", "%he", "%hE", "%hf", "%hF", "%hg", "%hG", "%ha", "%hA", "%hc",
    "%hC", "%hs", "%hS", "%hp", "%hn", "%h@", "%hhD", "%hhi", "%hhO", "%hhU", "%hhe", "%hhE",
    "%hhf", "%hhF", "%hhg", "%hhG", "%hha", "%hhA", "%hhc", "%hhC", "%hhs", "%hhS", "%hhp", "%hhn",
    "%hh@", "%lD", "%li", "%lO", "%lU", "%le", "%lE", "%lf", "%lF", "%lg", "%lG", "%la", "%lA",
    "%lc", "%lC", "%ls", "%lS", "%lp", "%ln", "%l@", "%llD", "%lli", "%llO", "%llU", "%lle",
    "%llE", "%llf", "%llF", "%llg", "%llG", "%lla", "%llA", "%llc", "%llC", "%lls", "%llS", "%llp",
    "%lln", "%ll@", "%qD", "%qi", "%qO", "%qU", "%qe", "%qE", "%qf", "%qF", "%qg", "%qG", "%qa",
    "%qA", "%qc", "%qC", "%qs", "%qS", "%qp", "%qn", "%q@", "%zD", "%zi", "%zO", "%zU", "%ze",
    "%zE", "%zf", "%zF", "%zg", "%zG", "%za", "%zA", "%zc", "%zC", "%zs", "%zS", "%zp", "%zn",
    "%z@", "%tD", "%ti", "%tO", "%tU", "%te", "%tE", "%tf", "%tF", "%tg", "%tG", "%ta", "%tA",
    "%tc", "%tC", "%ts", "%tS", "%tp", "%tn", "%t@", "%jD", "%ji", "%jO", "%jU", "%je", "%jE",
    "%jf", "%jF", "%jg", "%jG", "%ja", "%jA", "%jc", "%jC", "%js", "%jS", "%jp", "%jn", "%j@",
    "%Ld", "%LD", "%Li", "%Lo", "%LO", "%Lu", "%LU", "%Lx", "%LX", "%Lc", "%LC", "%Ls", "%LS",
    "%Lp", "%Ln", "%L@",
];
