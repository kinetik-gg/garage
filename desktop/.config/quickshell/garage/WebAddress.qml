pragma Singleton

import QtQuick

// Everything the launcher knows about the web: whether what was typed is an
// address, and whether it is the name of a site worth going straight to.
//
// Separate from LauncherPalette because it is a table and two parsers rather
// than any part of a panel, and because the table is the sort of thing that
// gets added to -- a hundred entries inline would be most of that file.
QtObject {
    id: web

    // Hosts a bare word is allowed to become. Deliberately a list rather than a
    // rule: "youtube" is a site and "meeting" is a word, and nothing about
    // their spelling separates them. Anything not named here needs a dot before
    // it is treated as an address, so typing a noun searches for it.
    //
    // Roughly the hundred most visited sites, by the names people actually type
    // for them. Where a name is ambiguous with an application the user is
    // likely to have installed -- "steam", "discord", "spotify" -- the row still
    // appears, but under the application, because rebuild() puts apps first.
    readonly property var sites: ({
        "google": "google.com",
        "youtube": "youtube.com",
        "facebook": "facebook.com",
        "instagram": "instagram.com",
        "whatsapp": "web.whatsapp.com",
        "x": "x.com",
        "twitter": "x.com",
        "wikipedia": "wikipedia.org",
        "reddit": "reddit.com",
        "chatgpt": "chatgpt.com",
        "openai": "openai.com",
        "claude": "claude.ai",
        "anthropic": "anthropic.com",
        "gemini": "gemini.google.com",
        "amazon": "amazon.com",
        "tiktok": "tiktok.com",
        "linkedin": "linkedin.com",
        "netflix": "netflix.com",
        "microsoft": "microsoft.com",
        "office": "office.com",
        "outlook": "outlook.com",
        "live": "live.com",
        "bing": "bing.com",
        "yahoo": "yahoo.com",
        "duckduckgo": "duckduckgo.com",
        "yandex": "yandex.com",
        "baidu": "baidu.com",
        "github": "github.com",
        "gitlab": "gitlab.com",
        "stackoverflow": "stackoverflow.com",
        "npm": "npmjs.com",
        "pypi": "pypi.org",
        "docker": "hub.docker.com",
        "huggingface": "huggingface.co",
        "kaggle": "kaggle.com",
        "vercel": "vercel.com",
        "netlify": "netlify.com",
        "cloudflare": "cloudflare.com",
        "aws": "aws.amazon.com",
        "azure": "portal.azure.com",
        "gcp": "console.cloud.google.com",
        "digitalocean": "digitalocean.com",
        "linear": "linear.app",
        "notion": "notion.so",
        "figma": "figma.com",
        "canva": "canva.com",
        "slack": "slack.com",
        "discord": "discord.com",
        "telegram": "web.telegram.org",
        "zoom": "zoom.us",
        "meet": "meet.google.com",
        "teams": "teams.microsoft.com",
        "trello": "trello.com",
        "asana": "asana.com",
        "jira": "atlassian.net",
        "atlassian": "atlassian.com",
        "dropbox": "dropbox.com",
        "drive": "drive.google.com",
        "gmail": "mail.google.com",
        "mail": "mail.google.com",
        "calendar": "calendar.google.com",
        "maps": "google.com/maps",
        "photos": "photos.google.com",
        "translate": "translate.google.com",
        "news": "news.google.com",
        "spotify": "open.spotify.com",
        "soundcloud": "soundcloud.com",
        "twitch": "twitch.tv",
        "vimeo": "vimeo.com",
        "disney": "disneyplus.com",
        "primevideo": "primevideo.com",
        "hbo": "max.com",
        "imdb": "imdb.com",
        "steam": "store.steampowered.com",
        "epic": "store.epicgames.com",
        "itch": "itch.io",
        "roblox": "roblox.com",
        "ebay": "ebay.com",
        "aliexpress": "aliexpress.com",
        "alibaba": "alibaba.com",
        "temu": "temu.com",
        "shopee": "shopee.co.id",
        "tokopedia": "tokopedia.com",
        "bukalapak": "bukalapak.com",
        "gojek": "gojek.com",
        "grab": "grab.com",
        "booking": "booking.com",
        "airbnb": "airbnb.com",
        "tripadvisor": "tripadvisor.com",
        "paypal": "paypal.com",
        "stripe": "dashboard.stripe.com",
        "wise": "wise.com",
        "coinbase": "coinbase.com",
        "binance": "binance.com",
        "medium": "medium.com",
        "substack": "substack.com",
        "quora": "quora.com",
        "pinterest": "pinterest.com",
        "tumblr": "tumblr.com",
        "archive": "archive.org",
        "coursera": "coursera.org",
        "udemy": "udemy.com",
        "khanacademy": "khanacademy.org",
        "duolingo": "duolingo.com",
        "bbc": "bbc.com",
        "cnn": "cnn.com",
        "nytimes": "nytimes.com",
        "theguardian": "theguardian.com",
        "detik": "detik.com",
        "kompas": "kompas.com",
        "arch": "archlinux.org",
        "aur": "aur.archlinux.org",
        "hyprland": "wiki.hypr.land",
        "protondb": "protondb.com",
        "speedtest": "speedtest.net"
    })

    // Last labels common enough that a bare host ending in one is almost
    // certainly meant as an address. This is not what decides whether something
    // parses -- see addressFor(), which accepts any syntactically valid host --
    // it decides what the launcher offers *first*.
    //
    // The distinction matters because several real top-level domains are also
    // everyday file extensions: .md, .sh, .zip and .mov are all delegated, so
    // "notes.md" is a valid address and almost never an intended one. Both rows
    // are offered either way; this only settles which of them Return takes.
    readonly property var commonSuffixes: [
        "com", "org", "net", "edu", "gov", "mil", "int", "io", "ai", "co",
        "dev", "app", "me", "tv", "fm", "gg", "xyz", "info", "biz",
        "online", "site", "cloud", "tech", "store", "blog", "wiki", "news",
        "id", "uk", "us", "ca", "au", "de", "fr", "nl", "es", "it", "se",
        "no", "fi", "dk", "pl", "cz", "ru", "ua", "jp", "kr", "cn", "tw",
        "hk", "sg", "my", "th", "vn", "ph", "in", "pk", "br", "mx", "ar",
        "cl", "za", "ng", "ke", "eg", "tr", "il", "ae", "sa", "ch", "at",
        "be", "pt", "gr", "ro", "hu", "ie", "nz"
    ]

    // Whether a host is well formed, by the rules a host actually has: labels of
    // letters, digits and hyphens, no leading or trailing hyphen, none empty,
    // none longer than 63 characters, and a final label that is alphabetic --
    // every delegated top-level domain is, and an internationalised one arrives
    // here already punycoded as xn--something.
    function validHost(host) {
        const name = String(host || "").replace(/:\d+$/, "");
        if (name === "" || name.length > 253)
            return false;
        const labels = name.split(".");
        if (labels.length < 2)
            return false;
        for (const label of labels) {
            if (!/^[a-z0-9]([a-z0-9-]{0,61}[a-z0-9])?$/i.test(label))
                return false;
        }
        return /^[a-z]{2,}$/i.test(labels[labels.length - 1])
            || /^xn--[a-z0-9-]+$/i.test(labels[labels.length - 1]);
    }

    // What the user typed, as something to open, or "" if it is not an address.
    //
    // Anything with a scheme is taken as given. Otherwise the part before the
    // first slash or query has to be a valid host, an IP, or localhost -- and
    // that is the whole test, so a site on a top-level domain nobody thought to
    // list still opens.
    function addressFor(input) {
        const text = String(input || "").trim();
        if (text === "" || /\s/.test(text))
            return "";
        if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text))
            return text;
        // A scheme with no slashes -- mailto:, magnet: -- belongs to the
        // desktop's handler for it rather than to a browser. A port is not one:
        // dots are legal in a scheme name, so "example.com:8080" parses as one
        // and was being handed to a handler that does not exist. Anything whose
        // colon is followed by digits and then nothing but a path is a host.
        if (/^[a-z][a-z0-9+.-]*:/i.test(text)
                && !/^[^:\/?#]+:\d+([\/?#]|$)/.test(text))
            return "";
        const host = text.split("/")[0].split("?")[0].split("#")[0];
        if (/^(\d{1,3}\.){3}\d{1,3}(:\d+)?$/.test(host)
                || /^localhost(:\d+)?$/i.test(host))
            return "http://" + text;
        return web.validHost(host) ? "https://" + text : "";
    }

    // Whether an address is one to offer ahead of a web search. An explicit
    // scheme, a machine on the network, or a familiar last label all say the
    // user meant to go somewhere; a bare word on an unusual top-level domain is
    // more often a filename, so the search goes first and this stays available
    // underneath it.
    function confident(input) {
        const text = String(input || "").trim();
        if (/^[a-z][a-z0-9+.-]*:\/\//i.test(text))
            return true;
        const host = text.split("/")[0].split("?")[0].split("#")[0];
        if (/^(\d{1,3}\.){3}\d{1,3}(:\d+)?$/.test(host)
                || /^localhost(:\d+)?$/i.test(host))
            return true;
        const labels = host.replace(/:\d+$/, "").split(".");
        const last = labels[labels.length - 1].toLowerCase();
        return web.commonSuffixes.indexOf(last) >= 0;
    }

    // The site a bare word names, or "" for anything not in the table. Exact
    // matches only: a prefix match would turn "n" into netflix and put a site
    // nobody named above the application they were reaching for.
    function siteFor(input) {
        const key = String(input || "").trim().toLowerCase();
        if (key === "" || /\s/.test(key))
            return "";
        // Entries in the table, and nothing the table inherited. `sites` is a
        // plain object, so "constructor" and "__proto__" find Object.prototype's
        // members -- and the launcher concatenated one of those into a URL and
        // offered "Open function Object() { [native code] }".
        const named = web.sites[key];
        return typeof named === "string" ? named : "";
    }

    // The host, for a label that says where this goes without the scheme and
    // trailing slash the user did not type.
    function displayHost(url) {
        return String(url || "").replace(/^[a-z][a-z0-9+.-]*:\/\//i, "")
            .replace(/\/$/, "");
    }
}
