.pragma library

// Pure query parsing for LauncherSources. Keeping this file free of QML objects
// makes every parser executable in a plain JavaScript test runner too.

var UNIT_DEFINITIONS = {
    mm: { group: "length", factor: 0.001, label: "mm" },
    cm: { group: "length", factor: 0.01, label: "cm" },
    m: { group: "length", factor: 1, label: "m" },
    km: { group: "length", factor: 1000, label: "km" },
    inch: { group: "length", factor: 0.0254, label: "in" },
    foot: { group: "length", factor: 0.3048, label: "ft" },
    yard: { group: "length", factor: 0.9144, label: "yd" },
    mile: { group: "length", factor: 1609.344, label: "mi" },

    mg: { group: "mass", factor: 0.000001, label: "mg" },
    g: { group: "mass", factor: 0.001, label: "g" },
    kg: { group: "mass", factor: 1, label: "kg" },
    ounce: { group: "mass", factor: 0.028349523125, label: "oz" },
    pound: { group: "mass", factor: 0.45359237, label: "lb" },
    stone: { group: "mass", factor: 6.35029318, label: "st" },
    tonne: { group: "mass", factor: 1000, label: "t" },

    celsius: { group: "temperature", label: "°C" },
    fahrenheit: { group: "temperature", label: "°F" },
    kelvin: { group: "temperature", label: "K" },

    ml: { group: "volume", factor: 0.001, label: "mL" },
    litre: { group: "volume", factor: 1, label: "L" },
    teaspoon: { group: "volume", factor: 0.00492892159375, label: "tsp" },
    tablespoon: { group: "volume", factor: 0.01478676478125, label: "tbsp" },
    cup: { group: "volume", factor: 0.2365882365, label: "cup" },
    pint: { group: "volume", factor: 0.473176473, label: "pt" },
    quart: { group: "volume", factor: 0.946352946, label: "qt" },
    gallon: { group: "volume", factor: 3.785411784, label: "gal" },

    ms: { group: "time", factor: 0.001, label: "ms" },
    second: { group: "time", factor: 1, label: "s" },
    minute: { group: "time", factor: 60, label: "min" },
    hour: { group: "time", factor: 3600, label: "h" },
    day: { group: "time", factor: 86400, label: "day" },

    mm2: { group: "area", factor: 0.000001, label: "mm²" },
    cm2: { group: "area", factor: 0.0001, label: "cm²" },
    m2: { group: "area", factor: 1, label: "m²" },
    km2: { group: "area", factor: 1000000, label: "km²" },
    in2: { group: "area", factor: 0.00064516, label: "in²" },
    ft2: { group: "area", factor: 0.09290304, label: "ft²" },
    acre: { group: "area", factor: 4046.8564224, label: "acre" },
    hectare: { group: "area", factor: 10000, label: "ha" },

    bit: { group: "data", factor: 0.125, label: "bit" },
    byte: { group: "data", factor: 1, label: "B" },
    kb: { group: "data", factor: 1000, label: "kB" },
    mb: { group: "data", factor: 1000000, label: "MB" },
    gb: { group: "data", factor: 1000000000, label: "GB" },
    tb: { group: "data", factor: 1000000000000, label: "TB" },
    kib: { group: "data", factor: 1024, label: "KiB" },
    mib: { group: "data", factor: 1048576, label: "MiB" },
    gib: { group: "data", factor: 1073741824, label: "GiB" },
    tib: { group: "data", factor: 1099511627776, label: "TiB" }
};

var UNIT_ALIASES = {
    "millimeter": "mm", "millimeters": "mm", "millimetre": "mm", "millimetres": "mm", "mm": "mm",
    "centimeter": "cm", "centimeters": "cm", "centimetre": "cm", "centimetres": "cm", "cm": "cm",
    "meter": "m", "meters": "m", "metre": "m", "metres": "m", "m": "m",
    "kilometer": "km", "kilometers": "km", "kilometre": "km", "kilometres": "km", "km": "km",
    "in": "inch", "inch": "inch", "inches": "inch",
    "ft": "foot", "foot": "foot", "feet": "foot",
    "yd": "yard", "yard": "yard", "yards": "yard",
    "mi": "mile", "mile": "mile", "miles": "mile",
    "mg": "mg", "milligram": "mg", "milligrams": "mg",
    "g": "g", "gram": "g", "grams": "g",
    "kg": "kg", "kilogram": "kg", "kilograms": "kg",
    "oz": "ounce", "ounce": "ounce", "ounces": "ounce",
    "lb": "pound", "lbs": "pound", "pound": "pound", "pounds": "pound",
    "st": "stone", "stone": "stone", "stones": "stone",
    "t": "tonne", "ton": "tonne", "tons": "tonne", "tonne": "tonne", "tonnes": "tonne",
    "c": "celsius", "°c": "celsius", "celsius": "celsius",
    "f": "fahrenheit", "°f": "fahrenheit", "fahrenheit": "fahrenheit",
    "k": "kelvin", "kelvin": "kelvin",
    "ml": "ml", "milliliter": "ml", "milliliters": "ml", "millilitre": "ml", "millilitres": "ml",
    "l": "litre", "liter": "litre", "liters": "litre", "litre": "litre", "litres": "litre",
    "tsp": "teaspoon", "teaspoon": "teaspoon", "teaspoons": "teaspoon",
    "tbsp": "tablespoon", "tablespoon": "tablespoon", "tablespoons": "tablespoon",
    "cup": "cup", "cups": "cup", "pt": "pint", "pint": "pint", "pints": "pint",
    "qt": "quart", "quart": "quart", "quarts": "quart", "gal": "gallon", "gallon": "gallon", "gallons": "gallon",
    "ms": "ms", "millisecond": "ms", "milliseconds": "ms",
    "s": "second", "sec": "second", "second": "second", "seconds": "second",
    "min": "minute", "minute": "minute", "minutes": "minute",
    "h": "hour", "hr": "hour", "hour": "hour", "hours": "hour",
    "d": "day", "day": "day", "days": "day",
    "mm2": "mm2", "mm²": "mm2", "cm2": "cm2", "cm²": "cm2", "m2": "m2", "m²": "m2",
    "km2": "km2", "km²": "km2", "in2": "in2", "in²": "in2", "ft2": "ft2", "ft²": "ft2",
    "acre": "acre", "acres": "acre", "ha": "hectare", "hectare": "hectare", "hectares": "hectare",
    "bit": "bit", "bits": "bit", "b": "byte", "byte": "byte", "bytes": "byte",
    "kb": "kb", "mb": "mb", "gb": "gb", "tb": "tb", "kib": "kib", "mib": "mib", "gib": "gib", "tib": "tib"
};

function cleanToken(value) {
    return String(value || "").trim().toLowerCase().replace(/\s+/g, " ");
}

function applicationRank(entry, needle) {
    if (entry.noDisplay)
        return -1;
    var query = cleanToken(needle);
    var name = cleanToken(entry.name);
    var rank = name.indexOf(query) === 0 ? 0
        : (name.indexOf(query) >= 0 ? 1
        : (cleanToken(entry.genericName).indexOf(query) >= 0 ? 2
        : (cleanToken(entry.comment).indexOf(query) >= 0 ? 3 : -1)));
    if (rank < 0)
        return -1;
    // Terminal=true desktop entries are valid launcher targets, but graphical
    // applications are the primary result type. Keep CLI entries available
    // after every matching desktop app instead of allowing an exact CLI name
    // to displace a graphical application's descriptive match.
    return rank + (entry.runInTerminal ? 10 : 0);
}

function formatNumber(value) {
    if (!isFinite(value))
        return "";
    var magnitude = Math.abs(value);
    var exact;
    if ((magnitude !== 0 && magnitude < 0.000000001) || magnitude >= 1000000000000000)
        exact = value.toExponential(8).replace(/\.?(?:0+)(e)/, "$1");
    else
        exact = (Math.round(value * 10000000000) / 10000000000).toFixed(10).replace(/\.?0+$/, "");
    var parts = exact.split(".");
    var sign = parts[0].charAt(0) === "-" ? "-" : "";
    var whole = sign === "" ? parts[0] : parts[0].slice(1);
    if (/^\d+$/.test(whole))
        whole = whole.replace(/\B(?=(\d{3})+(?!\d))/g, ",");
    return sign + whole + (parts.length > 1 ? "." + parts[1] : "");
}

function temperatureToCelsius(value, unit) {
    if (unit === "celsius") return value;
    if (unit === "fahrenheit") return (value - 32) * 5 / 9;
    return value - 273.15;
}

function celsiusToTemperature(value, unit) {
    if (unit === "celsius") return value;
    if (unit === "fahrenheit") return value * 9 / 5 + 32;
    return value + 273.15;
}

function unitConversion(input) {
    var match = /^\s*([+-]?(?:\d+(?:\.\d*)?|\.\d+))\s*([a-zA-Zµμ°²0-9]+(?:\s+[a-zA-Zµμ°²0-9]+)?)\s+(?:in|to)\s+([a-zA-Zµμ°²0-9]+(?:\s+[a-zA-Zµμ°²0-9]+)?)\s*$/i.exec(String(input || ""));
    if (!match)
        return null;
    var sourceKey = UNIT_ALIASES[cleanToken(match[2])];
    var targetKey = UNIT_ALIASES[cleanToken(match[3])];
    if (!sourceKey || !targetKey)
        return null;
    var source = UNIT_DEFINITIONS[sourceKey];
    var target = UNIT_DEFINITIONS[targetKey];
    if (source.group !== target.group)
        return null;
    var amount = Number(match[1]);
    var converted = source.group === "temperature"
        ? celsiusToTemperature(temperatureToCelsius(amount, sourceKey), targetKey)
        : amount * source.factor / target.factor;
    var title = formatNumber(amount) + " " + source.label + " = "
        + formatNumber(converted) + " " + target.label;
    return { kind: "unit", title: title,
        subtitle: source.group.charAt(0).toUpperCase() + source.group.slice(1) + " conversion — copy result",
        value: title };
}

var CURRENCY_ALIASES = {
    "$": "USD", "usd": "USD", "dollar": "USD", "dollars": "USD", "us dollar": "USD", "us dollars": "USD",
    "rp": "IDR", "idr": "IDR", "rupiah": "IDR", "indonesian rupiah": "IDR",
    "€": "EUR", "eur": "EUR", "euro": "EUR", "euros": "EUR",
    "£": "GBP", "gbp": "GBP", "pound": "GBP", "pounds": "GBP", "sterling": "GBP", "british pound": "GBP",
    "¥": "JPY", "jpy": "JPY", "yen": "JPY", "japanese yen": "JPY",
    "cny": "CNY", "rmb": "CNY", "yuan": "CNY", "renminbi": "CNY",
    "krw": "KRW", "won": "KRW", "korean won": "KRW",
    "sgd": "SGD", "singapore dollar": "SGD", "singapore dollars": "SGD",
    "myr": "MYR", "ringgit": "MYR", "malaysian ringgit": "MYR",
    "aud": "AUD", "australian dollar": "AUD", "cad": "CAD", "canadian dollar": "CAD",
    "nzd": "NZD", "new zealand dollar": "NZD", "hkd": "HKD", "hong kong dollar": "HKD",
    "inr": "INR", "rupee": "INR", "indian rupee": "INR", "thb": "THB", "baht": "THB",
    "php": "PHP", "philippine peso": "PHP", "chf": "CHF", "swiss franc": "CHF",
    "vnd": "VND", "dong": "VND", "vietnamese dong": "VND", "sar": "SAR", "riyal": "SAR",
    "aed": "AED", "dirham": "AED", "uae dirham": "AED", "zar": "ZAR", "south african rand": "ZAR"
};

function currencyCode(token) {
    var clean = cleanToken(token).replace(/[.,]$/, "");
    if (CURRENCY_ALIASES[clean])
        return CURRENCY_ALIASES[clean];
    return /^[a-z]{3}$/.test(clean) ? clean.toUpperCase() : "";
}

function currencyRequest(input) {
    var text = String(input || "");
    var number = "([+-]?(?:(?:\\d{1,3}(?:,\\d{3})+)|\\d+)(?:\\.\\d+)?|[+-]?\\.\\d+)";
    var symbolFirst = new RegExp("^\\s*([$€£¥])\\s*" + number + "\\s+(?:to|in)\\s+(.+?)\\s*$", "i").exec(text);
    var amountFirst = symbolFirst ? null
        : new RegExp("^\\s*" + number + "\\s+(.+?)\\s+(?:to|in)\\s+(.+?)\\s*$", "i").exec(text);
    var source;
    var target;
    var amountText;
    if (symbolFirst) {
        source = currencyCode(symbolFirst[1]);
        amountText = symbolFirst[2];
        target = currencyCode(symbolFirst[3]);
    } else if (amountFirst) {
        amountText = amountFirst[1];
        source = currencyCode(amountFirst[2]);
        target = currencyCode(amountFirst[3]);
    } else {
        return null;
    }
    if (!source || !target)
        return null;
    var amount = Number(amountText.replace(/,/g, ""));
    if (!isFinite(amount))
        return null;
    return { amount: amount, base: source, quote: target, pair: source + "/" + target };
}

function currencyResult(request, rate, date) {
    var converted = request.amount * rate;
    var title = formatNumber(request.amount) + " " + request.base + " = "
        + formatNumber(converted) + " " + request.quote;
    return { kind: "currency", title: title,
        subtitle: "Frankfurter" + (date ? " · " + date : "") + " — copy result", value: title };
}

var POWER_ACTIONS = [
    { action: "poweroff", title: "Shut Down…", aliases: ["shutdown", "shut down", "poweroff", "power off"] },
    { action: "restart", title: "Restart…", aliases: ["reboot", "restart"] },
    { action: "suspend", title: "Sleep…", aliases: ["sleep", "suspend"] },
    { action: "logout", title: "Log Out…", aliases: ["logout", "log out", "sign out"] },
    { action: "lock", title: "Lock Screen…", aliases: ["lock", "lock screen"] }
];

var MEDIA_ACTIONS = [
    { action: "play", title: "Play", aliases: ["play"], command: ["playerctl", "play"] },
    { action: "pause", title: "Pause", aliases: ["pause"], command: ["playerctl", "pause"] },
    { action: "stop", title: "Stop", aliases: ["stop"], command: ["playerctl", "stop"] },
    { action: "skip", title: "Skip Track", aliases: ["skip", "next"], command: ["playerctl", "next"] },
    { action: "mute", title: "Toggle Mute", aliases: ["mute", "unmute"], command: ["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"] }
];

function commandRows(input, prefixPattern, definitions, kindPrefix, subtitle) {
    var text = cleanToken(input);
    var prefixed = prefixPattern.exec(text);
    var term = prefixed ? cleanToken(prefixed[1]) : text;
    var rows = [];
    for (var index = 0; index < definitions.length; ++index) {
        var definition = definitions[index];
        var matches = prefixed && term === "";
        for (var alias = 0; !matches && alias < definition.aliases.length; ++alias)
            matches = definition.aliases[alias] === term
                || (term.length >= 2 && definition.aliases[alias].indexOf(term) === 0);
        if (matches)
            rows.push({ kind: kindPrefix + definition.action, title: definition.title,
                subtitle: subtitle, action: definition.action, command: definition.command || null });
    }
    return rows.length > 0 || (prefixed && term === "") ? rows : null;
}

function powerRows(input) {
    return commandRows(input, /^(?:power|system)(?:(?:\s*:\s*|\s+)(.*))?$/, POWER_ACTIONS,
        "session-", "System action — confirmation required");
}

function mediaRows(input) {
    return commandRows(input, /^(?:audio|media)(?:(?:\s*:\s*|\s+)(.*))?$/, MEDIA_ACTIONS,
        "media-", "Audio control");
}

function shellRows(input, dnd, caffeine, dark) {
    var text = cleanToken(input);
    if (text === "settings" || text === "system preferences" || text === "preferences")
        return [{ kind: "shell-settings", title: "Open System Preferences", subtitle: "Garage settings", action: "settings" }];
    if (text === "dnd" || text === "do not disturb")
        return [{ kind: "shell-dnd", title: dnd ? "Turn Off Do Not Disturb" : "Turn On Do Not Disturb",
            subtitle: dnd ? "Notifications are currently silenced" : "Silence notification popups", action: "dnd" }];
    if (text === "night" || text === "night shift")
        return [{ kind: "shell-night", title: "Toggle Night Shift", subtitle: "Enable or disable the scheduled warm display", action: "night" }];
    if (text === "light" || text === "dark" || text === "appearance" || text === "theme")
        return [{ kind: "shell-theme", title: dark ? "Switch to Light Appearance" : "Switch to Dark Appearance",
            subtitle: "Toggle the desktop color scheme", action: "theme" }];
    if (text === "caffeine" || text === "caffein")
        return [{ kind: "shell-caffeine", title: caffeine ? "Turn Caffeine Off" : "Turn Caffeine On",
            subtitle: caffeine ? "The display is being kept awake" : "Keep the display awake", action: "caffeine" }];
    return null;
}

function utilitySpec(input) {
    var text = String(input || "").trim();
    if (/^uuid\s*:?\s*$/i.test(text))
        return { key: "uuid", kind: "uuid", title: "", subtitle: "UUID v4 — copy result" };
    var random = /^rand\s*\(\s*(\d+)\s*\)\s*$/i.exec(text);
    if (random) {
        var length = Number(random[1]);
        if (length < 1 || length > 128)
            return { key: "rand-error", kind: "error", title: "Use rand(1) through rand(128)", subtitle: "Random digit length is limited to 128" };
        return { key: "rand:" + length, kind: "random", length: length,
            title: "", subtitle: length + " random digits — copy result" };
    }
    return /^rand\s*\(/i.test(text)
        ? { key: "rand-error", kind: "error", title: "Use rand(n)", subtitle: "Example: rand(16)" }
        : null;
}

function randomDigits(length) {
    var result = "";
    for (var index = 0; index < length; ++index)
        result += String(Math.floor(Math.random() * 10));
    return result;
}

function uuidV4() {
    var output = "";
    var pattern = "xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx";
    for (var index = 0; index < pattern.length; ++index) {
        var token = pattern.charAt(index);
        if (token !== "x" && token !== "y") {
            output += token;
            continue;
        }
        var random = Math.floor(Math.random() * 16);
        output += (token === "x" ? random : (random & 3) | 8).toString(16);
    }
    return output;
}

function generateUtility(spec) {
    return spec.kind === "uuid" ? uuidV4() : randomDigits(spec.length);
}

function compactDuration(milliseconds) {
    var seconds = Math.max(0, Math.round(Number(milliseconds) / 1000));
    var parts = [];
    var units = [[86400, "d"], [3600, "h"], [60, "m"], [1, "s"]];
    for (var index = 0; index < units.length; ++index) {
        var amount = Math.floor(seconds / units[index][0]);
        if (amount > 0 || (units[index][0] === 1 && parts.length === 0)) {
            parts.push(String(amount) + units[index][1]);
            seconds -= amount * units[index][0];
        }
    }
    return parts.join(" ");
}

function clockDuration(milliseconds, tenths) {
    var total = Math.max(0, Math.floor(Number(milliseconds)));
    var hours = Math.floor(total / 3600000);
    var minutes = Math.floor(total % 3600000 / 60000);
    var seconds = Math.floor(total % 60000 / 1000);
    var fraction = Math.floor(total % 1000 / 100);
    function pad(value) { return value < 10 ? "0" + value : String(value); }
    var text = hours > 0
        ? String(hours) + ":" + pad(minutes) + ":" + pad(seconds)
        : pad(minutes) + ":" + pad(seconds);
    return tenths ? text + "." + fraction : text;
}

function timerSpec(input) {
    var match = /^\s*timer(?:\s+(.*?))?\s*$/i.exec(String(input || ""));
    if (!match)
        return null;
    var argument = String(match[1] || "").trim();
    if (argument === "")
        return { mode: "list" };
    if (/^cancel$/i.test(argument))
        return { mode: "cancel" };

    var remaining = argument;
    var milliseconds = 0;
    var consumed = false;
    var unitMilliseconds = { d: 86400000, h: 3600000, m: 60000, s: 1000 };
    while (true) {
        var duration = /^(\d+(?:\.\d+)?)\s*([dhms])(?:\s+|$)/i.exec(remaining);
        if (!duration)
            break;
        consumed = true;
        milliseconds += Number(duration[1]) * unitMilliseconds[duration[2].toLowerCase()];
        remaining = remaining.slice(duration[0].length);
    }
    if (!consumed || !isFinite(milliseconds) || milliseconds < 1000
            || milliseconds > 7 * 86400000)
        return { mode: "error", title: "Use timer 10m or timer 1h 30m Tea",
            subtitle: "Timers can run from one second through seven days" };
    var label = remaining.trim() || compactDuration(milliseconds) + " timer";
    return { mode: "start", durationMs: Math.round(milliseconds), label: label };
}

function stopwatchSpec(input) {
    var match = /^\s*stopwatch(?:\s+(.*?))?\s*$/i.exec(String(input || ""));
    if (!match)
        return null;
    var action = cleanToken(match[1] || "");
    if (action === "")
        return { action: "list" };
    if (["start", "pause", "resume", "lap", "reset"].indexOf(action) >= 0)
        return { action: action };
    return { action: "error", title: "Use stopwatch start, pause, resume, lap, or reset",
        subtitle: "Stopwatch controls" };
}

function fileSearchQuery(input) {
    var match = /^\s*(?:file|files)(?:(?:\s*:\s*|\s+)(.*?))?\s*$/i.exec(String(input || ""));
    return match ? String(match[1] || "").trim() : null;
}

function isExclusiveQuery(input) {
    return unitConversion(input) !== null
        || currencyRequest(input) !== null
        || utilitySpec(input) !== null
        || emojiRows(input, 1) !== null
        || killQuery(input) !== null
        || sshSpec(input) !== null
        || powerRows(input) !== null
        || mediaRows(input) !== null
        || shellRows(input, false, false, false) !== null
        || timerSpec(input) !== null
        || stopwatchSpec(input) !== null
        || fileSearchQuery(input) !== null;
}

var EMOJI = [
    ["❤️", "red heart", "love romance like favorite"], ["🧡", "orange heart", "love warm"],
    ["💛", "yellow heart", "love friendship happy"], ["💚", "green heart", "love nature"],
    ["💙", "blue heart", "love trust"], ["💜", "purple heart", "love affection"],
    ["🖤", "black heart", "love dark sorrow"], ["🤍", "white heart", "love pure"],
    ["💕", "two hearts", "love romance affection"], ["💖", "sparkling heart", "love excited"],
    ["💘", "heart with arrow", "love cupid romance"], ["💝", "heart with ribbon", "love gift valentine"],
    ["🥰", "smiling face with hearts", "love affection happy crush"], ["😍", "heart eyes", "love crush adore"],
    ["😘", "face blowing a kiss", "love kiss affection"], ["😊", "smiling face", "happy blush friendly"],
    ["😀", "grinning face", "happy smile joy"], ["😂", "face with tears of joy", "laugh funny cry"],
    ["🤣", "rolling on the floor laughing", "laugh funny rofl"], ["🙂", "slightly smiling face", "smile okay"],
    ["😉", "winking face", "wink joke playful"], ["😎", "smiling face with sunglasses", "cool confident sun"],
    ["🤩", "star struck", "excited wow amazing"], ["🥳", "partying face", "party celebrate birthday"],
    ["😭", "loudly crying face", "sad cry tears"], ["😢", "crying face", "sad tear"],
    ["😡", "enraged face", "angry mad rage"], ["🤔", "thinking face", "think question unsure"],
    ["🫡", "saluting face", "salute respect yes"], ["🫠", "melting face", "hot embarrassed disappear"],
    ["😴", "sleeping face", "sleep tired night"], ["🤯", "exploding head", "mind blown shocked wow"],
    ["👍", "thumbs up", "yes good approve like"], ["👎", "thumbs down", "no bad disapprove dislike"],
    ["👏", "clapping hands", "applause congrats praise"], ["🙏", "folded hands", "please thanks pray hope"],
    ["🤝", "handshake", "deal agreement hello"], ["💪", "flexed biceps", "strong muscle power"],
    ["✌️", "victory hand", "peace victory two"], ["🤞", "crossed fingers", "luck hope"],
    ["👋", "waving hand", "hello goodbye wave"], ["👌", "ok hand", "okay good perfect"],
    ["🔥", "fire", "hot lit flame popular"], ["✨", "sparkles", "shine magic clean new"],
    ["⭐", "star", "favorite rating night"], ["🎉", "party popper", "celebrate party congratulations"],
    ["🎂", "birthday cake", "birthday celebrate dessert"], ["🎁", "wrapped gift", "present birthday surprise"],
    ["✅", "check mark button", "yes done success correct"], ["❌", "cross mark", "no wrong error cancel"],
    ["⚠️", "warning", "alert caution danger"], ["ℹ️", "information", "info help"],
    ["🚀", "rocket", "launch fast space ship"], ["💡", "light bulb", "idea insight lamp"],
    ["🎯", "bullseye", "target goal accurate"], ["🏆", "trophy", "winner award success"],
    ["💻", "laptop", "computer code work"], ["⌨️", "keyboard", "type computer"],
    ["📱", "mobile phone", "phone device call"], ["📌", "pushpin", "pin location important"],
    ["📎", "paperclip", "attach attachment office"], ["🔒", "locked", "lock secure private"],
    ["🔑", "key", "password unlock access"], ["🔍", "magnifying glass", "search find zoom"],
    ["🐶", "dog face", "dog puppy pet animal"], ["🐱", "cat face", "cat kitten pet animal"],
    ["🐼", "panda", "bear animal cute"], ["🦊", "fox", "animal clever"],
    ["🦁", "lion", "animal king brave"], ["🐸", "frog", "animal green"],
    ["🦋", "butterfly", "insect nature beautiful"], ["🌸", "cherry blossom", "flower spring pink"],
    ["🌹", "rose", "flower love romance"], ["🌻", "sunflower", "flower sun yellow"],
    ["🌞", "sun with face", "sun day bright happy"], ["🌙", "crescent moon", "night sleep dark"],
    ["☕", "hot beverage", "coffee tea caffeine drink"], ["🍵", "teacup", "tea drink warm"],
    ["🍕", "pizza", "food slice"], ["🍔", "hamburger", "food burger"],
    ["🍜", "steaming bowl", "noodle ramen food"], ["🍚", "cooked rice", "rice food"],
    ["🍰", "shortcake", "cake dessert sweet"], ["🍫", "chocolate bar", "chocolate sweet dessert"],
    ["🍺", "beer mug", "beer drink cheers"], ["🥂", "clinking glasses", "cheers celebrate drink"],
    ["⚽", "soccer ball", "football sport"], ["🏀", "basketball", "sport ball"],
    ["🎮", "video game", "game controller play"], ["🎵", "musical note", "music song audio"],
    ["🎬", "clapper board", "movie film cinema"], ["📷", "camera", "photo picture"],
    ["✈️", "airplane", "travel flight plane"], ["🚗", "automobile", "car drive vehicle"],
    ["🏠", "house", "home building"], ["🌍", "globe", "earth world travel"],
    ["🇮🇩", "Indonesia flag", "indonesia flag country merah putih"], ["🇺🇸", "United States flag", "usa america flag country"]
];

function emojiRows(input, limit) {
    var match = /^\s*emoji(?:(?:\s*:\s*|\s+)(.*?))?\s*$/i.exec(String(input || ""));
    if (!match)
        return null;
    var query = cleanToken(match[1] || "");
    var tokens = query === "" ? [] : query.split(" ");
    var matches = [];
    for (var index = 0; index < EMOJI.length; ++index) {
        var item = EMOJI[index];
        var haystack = cleanToken(item[1] + " " + item[2]);
        var accepted = true;
        for (var token = 0; token < tokens.length; ++token)
            accepted = accepted && haystack.indexOf(tokens[token]) >= 0;
        if (!accepted)
            continue;
        var score = query !== "" && cleanToken(item[1]) === query ? 0
            : (query !== "" && cleanToken(item[1]).indexOf(query) === 0 ? 1 : 2);
        matches.push({ score: score, item: item, order: index });
    }
    matches.sort(function(left, right) {
        return left.score - right.score || left.order - right.order;
    });
    return matches.slice(0, limit).map(function(match) {
        return { kind: "emoji", title: match.item[0] + "  " + match.item[1],
            subtitle: match.item[2] + " — copy emoji", value: match.item[0] };
    });
}

function killQuery(input) {
    var match = /^\s*kill(?:(?:\s*:\s*|\s+)(.*?))?\s*$/i.exec(String(input || ""));
    return match ? cleanToken(match[1] || "") : null;
}

function parseProcessList(text) {
    var rows = [];
    var lines = String(text || "").split("\n");
    for (var index = 0; index < lines.length; ++index) {
        var match = /^\s*(\d+)\s+(\S+)(?:\s+(.*))?$/.exec(lines[index]);
        if (!match)
            continue;
        rows.push({ pid: Number(match[1]), name: match[2], command: String(match[3] || match[2]) });
    }
    return rows;
}

function fuzzyScore(candidate, needle) {
    var haystack = cleanToken(candidate);
    var query = cleanToken(needle);
    if (query === "")
        return 0;
    var at = 0;
    var previous = -2;
    var score = 0;
    for (var index = 0; index < query.length; ++index) {
        var found = haystack.indexOf(query.charAt(index), at);
        if (found < 0)
            return null;
        score += found;
        if (found === previous + 1)
            score -= 4;
        if (found === 0 || /[\s/_.-]/.test(haystack.charAt(found - 1)))
            score -= 3;
        previous = found;
        at = found + 1;
    }
    return score + haystack.length / 1000;
}

function processRows(query, processes, limit) {
    if (query === "")
        return [{ kind: "status", title: "Type a process name after kill", subtitle: "Processes are matched fuzzily" }];
    var matches = [];
    for (var index = 0; index < processes.length; ++index) {
        var process = processes[index];
        var score = fuzzyScore(process.name + " " + process.command, query);
        if (score !== null)
            matches.push({ score: score, process: process });
    }
    matches.sort(function(left, right) {
        return left.score - right.score || left.process.name.localeCompare(right.process.name)
            || left.process.pid - right.process.pid;
    });
    return matches.slice(0, limit).map(function(match) {
        var process = match.process;
        return { kind: "process", title: process.name + "  ·  PID " + process.pid,
            subtitle: process.command, pid: process.pid };
    });
}

function sshSpec(input) {
    var text = String(input || "").trim();
    if (!/^ssh(?:\s|$)/i.test(text))
        return null;
    var match = /^ssh\s+((?:[A-Za-z0-9_][A-Za-z0-9._-]*@)?(?:[A-Za-z0-9](?:[A-Za-z0-9.-]*[A-Za-z0-9])?|\[[0-9A-Fa-f:]+\]))$/i.exec(text);
    if (!match)
        return { kind: "error", title: "Use ssh user@host", subtitle: "Options and shell syntax are not accepted" };
    return { kind: "ssh", title: "Connect to " + match[1], subtitle: "Open SSH in the default terminal", target: match[1] };
}
