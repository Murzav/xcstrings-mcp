// Generated from Unicode CLDR 48.2.1; provenance and license: data/README.md.
use super::PluralCategory;

pub(super) fn lookup(locale: &str) -> Option<&'static [PluralCategory]> {
    use PluralCategory::*;
    match locale {
        "af" | "ak" | "am" | "an" | "as" | "asa" | "ast" | "az" | "bal" | "bem" | "bez" | "bg"
        | "bho" | "bn" | "brx" | "ce" | "ceb" | "cgg" | "chr" | "ckb" | "csw" | "da" | "de"
        | "doi" | "dv" | "ee" | "el" | "en" | "eo" | "et" | "eu" | "fa" | "ff" | "fi" | "fil"
        | "fo" | "fur" | "fy" | "gl" | "gsw" | "gu" | "guw" | "ha" | "haw" | "hi" | "hu" | "hy"
        | "ia" | "ie" | "io" | "is" | "jgo" | "jmc" | "ka" | "kab" | "kaj" | "kcg" | "kk"
        | "kkj" | "kl" | "kn" | "kok" | "kok-latn" | "ks" | "ksb" | "ku" | "ky" | "lb" | "lg"
        | "lij" | "ln" | "mas" | "mg" | "mgo" | "mk" | "ml" | "mn" | "mr" | "nah" | "nb" | "nd"
        | "ne" | "nl" | "nn" | "nnh" | "no" | "nr" | "nso" | "ny" | "nyn" | "om" | "or" | "os"
        | "pa" | "pap" | "pcm" | "ps" | "rm" | "rof" | "rwk" | "saq" | "sc" | "sd" | "sdh"
        | "seh" | "si" | "sn" | "so" | "sq" | "ss" | "ssy" | "st" | "sv" | "sw" | "syr" | "ta"
        | "te" | "teo" | "ti" | "tig" | "tk" | "tl" | "tn" | "tr" | "ts" | "tzm" | "ug" | "ur"
        | "uz" | "ve" | "vo" | "vun" | "wa" | "wae" | "xh" | "xog" | "yi" | "zu" => {
            Some(&[One, Other])
        }
        "ar" | "ars" | "cy" | "kw" => Some(&[Zero, One, Two, Few, Many, Other]),
        "be" | "cs" | "lt" | "pl" | "ru" | "sk" | "uk" => Some(&[One, Few, Many, Other]),
        "blo" | "cv" | "ksh" | "lag" | "lv" | "prg" => Some(&[Zero, One, Other]),
        "bm" | "bo" | "dz" | "hnj" | "id" | "ig" | "ii" | "ja" | "jbo" | "jv" | "jw" | "kde"
        | "kea" | "km" | "ko" | "lkt" | "lo" | "ms" | "my" | "nqo" | "osa" | "sah" | "ses"
        | "sg" | "su" | "th" | "to" | "tpi" | "und" | "vi" | "wo" | "yo" | "yue" | "zh" => {
            Some(&[Other])
        }
        "br" | "ga" | "gv" | "mt" | "sgs" => Some(&[One, Two, Few, Many, Other]),
        "bs" | "hr" | "mo" | "ro" | "sh" | "shi" | "sr" => Some(&[One, Few, Other]),
        "ca" | "es" | "fr" | "it" | "lld" | "pt" | "pt-pt" | "scn" | "vec" => {
            Some(&[One, Many, Other])
        }
        "dsb" | "gd" | "hsb" | "sl" => Some(&[One, Two, Few, Other]),
        "he" | "iu" | "naq" | "sat" | "se" | "sma" | "smi" | "smj" | "smn" | "sms" => {
            Some(&[One, Two, Other])
        }
        _ => None,
    }
}
