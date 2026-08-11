pragma Singleton

import QtQuick

// A small recursive-descent parser for the launcher's calculator.
//
// Deliberately not eval(): the input is whatever the user types, and handing
// that to the JS engine would let a launcher query execute arbitrary QML.
QtObject {
    id: calculator

    // Recognised only when the expression parses cleanly *and* actually computes
    // something. Without the operator test a bare "2020" would stop being an app
    // search and turn into a calculator hit.
    function evaluate(input) {
        const text = String(input || "").trim();
        if (text === "" || !/[-+*/%^]/.test(text))
            return null;
        if (!/^[0-9+\-*/%^(). \t]+$/.test(text))
            return null;

        const state = { text: text, at: 0 };
        let value;
        try {
            value = parseSum(state);
        } catch (error) {
            return null;
        }
        skipSpace(state);
        if (state.at !== state.text.length)
            return null;
        if (!isFinite(value))
            return null;
        return format(value);
    }

    // Grouped in thousands, because the answer is the whole point of the row and
    // an unbroken run of digits is not one anybody can read at a glance.
    // Separators go on the integer part only -- the fraction is a position, not
    // a magnitude -- and the locale decides the character, so this reads the way
    // the rest of the desktop's numbers do rather than assuming a comma.
    function format(value) {
        const rounded = Math.round(value * 1e10) / 1e10;
        const exact = Number.isInteger(rounded)
            ? String(rounded) : String(parseFloat(rounded.toFixed(10)));
        // Anything Number cannot round-trip is left as it came out: past 2^53
        // the digits are already an approximation and grouping them would only
        // make the approximation look deliberate.
        if (!isFinite(rounded) || Math.abs(rounded) >= Number.MAX_SAFE_INTEGER
                || exact.indexOf("e") >= 0)
            return exact;
        const negative = exact.charAt(0) === "-";
        const digits = negative ? exact.slice(1) : exact;
        const point = digits.indexOf(".");
        const whole = point < 0 ? digits : digits.slice(0, point);
        // The locale's point, not the one the parser happened to accept. Under a
        // locale that groups with "." and points with "," -- most of Europe --
        // keeping the typed character produced 1.234.567.89.
        const fraction = point < 0
            ? "" : Qt.locale().decimalPoint + digits.slice(point + 1);
        return (negative ? "-" : "")
            + Number(whole).toLocaleString(Qt.locale(), "f", 0) + fraction;
    }

    function skipSpace(state) {
        while (state.at < state.text.length && /\s/.test(state.text[state.at]))
            state.at++;
    }

    function peek(state) {
        skipSpace(state);
        return state.at < state.text.length ? state.text[state.at] : "";
    }

    function parseSum(state) {
        let value = parseProduct(state);
        for (;;) {
            const op = peek(state);
            if (op !== "+" && op !== "-")
                return value;
            state.at++;
            const rhs = parseProduct(state);
            value = op === "+" ? value + rhs : value - rhs;
        }
    }

    function parseProduct(state) {
        let value = parsePower(state);
        for (;;) {
            const op = peek(state);
            if (op !== "*" && op !== "/" && op !== "%")
                return value;
            state.at++;
            const rhs = parsePower(state);
            if ((op === "/" || op === "%") && rhs === 0)
                throw new Error("divide by zero");
            value = op === "*" ? value * rhs : op === "/" ? value / rhs : value % rhs;
        }
    }

    // Right associative, so 2^3^2 is 512 rather than 64.
    function parsePower(state) {
        const base = parseUnary(state);
        if (peek(state) !== "^")
            return base;
        state.at++;
        return Math.pow(base, parsePower(state));
    }

    function parseUnary(state) {
        const op = peek(state);
        if (op === "-") {
            state.at++;
            return -parseUnary(state);
        }
        if (op === "+") {
            state.at++;
            return parseUnary(state);
        }
        return parseAtom(state);
    }

    function parseAtom(state) {
        if (peek(state) === "(") {
            state.at++;
            const value = parseSum(state);
            if (peek(state) !== ")")
                throw new Error("unbalanced");
            state.at++;
            return value;
        }

        skipSpace(state);
        const match = /^[0-9]*\.?[0-9]+/.exec(state.text.slice(state.at));
        if (!match)
            throw new Error("expected a number");
        state.at += match[0].length;
        return parseFloat(match[0]);
    }
}
