import Quickshell
import Quickshell.Io
import QtQuick
import "LauncherExtras.js" as LauncherExtras

// Async data and generated values for LauncherPalette. The palette itself owns
// only the stable ListModel; this scope waits for external data and hands over a
// complete set of plain-JS rows when it is ready.
Scope {
    id: sources

    signal changed()

    property var currencyRates: ({})
    property string currencyWantedPair: ""
    property string generatedKey: ""
    property string generatedValue: ""
    property var processes: []
    property bool processesStarted: false
    property bool processesReady: false
    property string fileWantedQuery: ""
    property string fileRequestQuery: ""
    property var fileResults: []
    property bool fileReady: false
    property var clipboardItems: []
    property bool clipboardStarted: false
    property bool clipboardReady: false
    property string clipboardError: ""

    function append(target, rows) {
        if (rows === null)
            return false;
        for (let index = 0; index < rows.length; ++index)
            target.push(rows[index]);
        return true;
    }

    function utilityRow(spec) {
        if (spec.kind === "error")
            return spec;
        if (sources.generatedKey !== spec.key) {
            sources.generatedKey = spec.key;
            sources.generatedValue = LauncherExtras.generateUtility(spec);
        }
        return { kind: spec.kind, title: sources.generatedValue,
            subtitle: spec.subtitle, value: sources.generatedValue };
    }

    function requestCurrency(spec) {
        sources.currencyWantedPair = spec.pair;
        if (spec.base === spec.quote || sources.currencyRates[spec.pair] !== undefined)
            return;
        sources.startCurrencyRequest(spec);
    }

    function startCurrencyRequest(spec) {
        if (currencyProcess.running)
            return;
        let request = spec;
        if (request === undefined || request === null) {
            const pair = sources.currencyWantedPair.split("/");
            if (pair.length !== 2 || sources.currencyRates[sources.currencyWantedPair] !== undefined)
                return;
            request = { base: pair[0], quote: pair[1], pair: sources.currencyWantedPair };
        }
        currencyProcess.requestPair = request.pair;
        currencyProcess.command = ["curl", "--fail", "--silent", "--show-error",
            "--max-time", "8", "https://api.frankfurter.dev/v2/rate/"
                + request.base + "/" + request.quote];
        currencyProcess.running = true;
    }

    function finishCurrency(pair, output) {
        let value;
        try {
            const payload = JSON.parse(String(output || ""));
            const rate = Number(payload.rate);
            if (!isFinite(rate) || rate <= 0)
                throw new Error("invalid rate");
            value = { rate: rate, date: String(payload.date || "") };
        } catch (error) {
            value = { error: "Frankfurter did not return a usable rate" };
        }
        const next = Object.assign({}, sources.currencyRates);
        next[pair] = value;
        sources.currencyRates = next;
        if (sources.currencyWantedPair === pair)
            sources.changed();
    }

    function currencyRow(spec) {
        sources.requestCurrency(spec);
        if (spec.base === spec.quote)
            return LauncherExtras.currencyResult(spec, 1, "");
        const cached = sources.currencyRates[spec.pair];
        if (cached === undefined)
            return { kind: "status", title: "Fetching " + spec.base + "/" + spec.quote + " rate…",
                subtitle: "Frankfurter currency data" };
        if (cached.error)
            return { kind: "currency-error", title: "Unable to convert " + spec.base + " to " + spec.quote,
                subtitle: cached.error + " — select to retry", currency: spec };
        return LauncherExtras.currencyResult(spec, cached.rate, cached.date);
    }

    function retryCurrency(spec) {
        const next = Object.assign({}, sources.currencyRates);
        delete next[spec.pair];
        sources.currencyRates = next;
        sources.requestCurrency(spec);
        sources.changed();
    }

    function ensureProcesses() {
        if (sources.processesStarted)
            return;
        sources.processesStarted = true;
        processProbe.running = true;
    }

    function wantedFileTerm(input) {
        const explicit = LauncherExtras.fileSearchQuery(input);
        if (explicit !== null)
            return explicit;
        return LauncherExtras.isExclusiveQuery(input) ? "" : String(input || "").trim();
    }

    // Start against the field immediately, ahead of the launcher's debounce.
    // The short-lived SQLite reader normally finishes before the visible model
    // commits; if an older query is still running, its answer is ignored and the
    // newest one starts as soon as the process becomes free.
    function prepareFiles(input) {
        const term = sources.wantedFileTerm(input);
        if (term === sources.fileWantedQuery
                && (term === "" || sources.fileReady || fileProcess.running))
            return;
        sources.fileWantedQuery = term;
        // Keep the previous completed answer while the next SQLite query runs.
        // LauncherPalette also keeps its visible model, so typing never clears
        // delegates just to refill them a few milliseconds later.
        sources.fileReady = term === "";
        if (term !== "" && !fileProcess.running)
            sources.startFileRequest();
    }

    function startFileRequest() {
        if (fileProcess.running || sources.fileWantedQuery === "")
            return;
        sources.fileRequestQuery = sources.fileWantedQuery;
        fileProcess.command = [GaragePaths.fileIndex,
            "search", sources.fileRequestQuery, String(8)];
        fileProcess.running = true;
    }

    function finishFileRequest(query, output) {
        if (query !== sources.fileWantedQuery)
            return;
        let rows = [];
        try {
            const response = JSON.parse(String(output || ""));
            if (!response.ok)
                throw new Error(response.error || "file search failed");
            rows = response.data && Array.isArray(response.data.rows)
                ? response.data.rows : [];
        } catch (error) {
            rows = [];
        }
        sources.fileResults = rows;
        sources.fileReady = true;
        sources.changed();
    }

    function filePending(input) {
        const term = sources.wantedFileTerm(input);
        return term !== "" && term === sources.fileWantedQuery && !sources.fileReady;
    }

    function fileRowsFor(input, limit, explicit) {
        const requested = LauncherExtras.fileSearchQuery(input);
        if (explicit && requested === null)
            return null;
        const term = requested !== null ? requested : String(input || "").trim();
        if (term === "")
            return explicit ? [{ kind: "status", title: "Search indexed files",
                subtitle: "Type a name after file" }] : [];
        if (term !== sources.fileWantedQuery || !sources.fileReady)
            return explicit ? [{ kind: "status", title: "Searching indexed files…",
                subtitle: term }] : [];
        return sources.fileResults.slice(0, limit).map(row => ({
            kind: String(row.kind || "file"), title: String(row.title || ""),
            subtitle: String(row.subtitle || ""), path: String(row.path || "")
        }));
    }

    function ensureClipboard() {
        if (sources.clipboardStarted)
            return;
        sources.clipboardStarted = true;
        clipboardProcess.running = true;
    }

    function finishClipboard(output) {
        sources.clipboardItems = LauncherExtras.parseClipboardList(output);
        sources.clipboardReady = true;
        sources.changed();
    }

    function clipboardRowsFor(input, limit, dedicated) {
        const query = dedicated ? String(input || "").trim()
            : LauncherExtras.clipboardQuery(input);
        if (query === null)
            return null;
        sources.ensureClipboard();
        if (!sources.clipboardReady)
            return [{ kind: "status", title: "Loading clipboard history…",
                subtitle: "cliphist" }];
        if (sources.clipboardError !== "")
            return [{ kind: "error", title: "Clipboard history unavailable",
                subtitle: sources.clipboardError }];
        if (sources.clipboardItems.length === 0)
            return [{ kind: "status", title: "Clipboard history is empty",
                subtitle: "Copy something, then open this mode again" }];
        const rows = LauncherExtras.clipboardRows(
            sources.clipboardItems, query, limit);
        return rows.length > 0 ? rows
            : [{ kind: "status", title: "No matching clipboard items",
                subtitle: query }];
    }

    // Returns every launcher-specific row and whether normal app/web searching
    // should stand down for this query. Recognised command syntaxes are exclusive
    // so an emoji or PID search is not followed by a web-search row for itself.
    function rowsFor(input, dnd, caffeine, dark, limit, clipboardMode) {
        const rows = [];
        let exclusive = false;

        const clipboard = sources.clipboardRowsFor(input, limit, clipboardMode);
        if (clipboard !== null) {
            sources.append(rows, clipboard);
            return { rows: rows, exclusive: true };
        }

        const unit = LauncherExtras.unitConversion(input);
        if (unit !== null) {
            rows.push(unit);
            exclusive = true;
        }

        const currency = unit === null ? LauncherExtras.currencyRequest(input) : null;
        if (currency !== null) {
            rows.push(sources.currencyRow(currency));
            exclusive = true;
        } else {
            sources.currencyWantedPair = "";
        }

        const utility = LauncherExtras.utilitySpec(input);
        if (utility !== null) {
            rows.push(sources.utilityRow(utility));
            exclusive = true;
        } else {
            sources.generatedKey = "";
            sources.generatedValue = "";
        }

        const emoji = LauncherExtras.emojiRows(input, limit);
        if (emoji !== null) {
            if (emoji.length > 0)
                sources.append(rows, emoji);
            else
                rows.push({ kind: "status", title: "No matching emoji", subtitle: "Try another keyword after emoji" });
            exclusive = true;
        }

        const processQuery = LauncherExtras.killQuery(input);
        if (processQuery !== null) {
            sources.ensureProcesses();
            if (!sources.processesReady)
                rows.push({ kind: "status", title: "Loading processes…", subtitle: "Preparing fuzzy PID search" });
            else {
                const processMatches = LauncherExtras.processRows(processQuery, sources.processes, limit);
                if (processMatches.length > 0)
                    sources.append(rows, processMatches);
                else
                    rows.push({ kind: "status", title: "No matching process", subtitle: processQuery });
            }
            exclusive = true;
        }

        const ssh = LauncherExtras.sshSpec(input);
        if (ssh !== null) {
            rows.push(ssh);
            exclusive = true;
        }

        const clock = TimerService.rowsFor(input);
        if (clock !== null) {
            sources.append(rows, clock);
            exclusive = true;
        }

        const explicitFiles = sources.fileRowsFor(input, limit, true);
        if (explicitFiles !== null) {
            sources.append(rows, explicitFiles);
            exclusive = true;
        }

        exclusive = sources.append(rows, LauncherExtras.powerRows(input)) || exclusive;
        exclusive = sources.append(rows, LauncherExtras.mediaRows(input)) || exclusive;
        exclusive = sources.append(rows, LauncherExtras.shellRows(input, dnd, caffeine, dark)) || exclusive;

        return { rows: rows, exclusive: exclusive };
    }

    Process {
        id: clipboardProcess

        command: ["cliphist", "list"]
        onExited: exitCode => {
            if (exitCode === 0)
                return;
            sources.clipboardError = "cliphist list exited (" + exitCode + ")";
            sources.clipboardReady = true;
            sources.changed();
        }
        stdout: StdioCollector {
            onStreamFinished: sources.finishClipboard(text)
        }
        stderr: StdioCollector {}
    }

    Process {
        id: currencyProcess
        property string requestPair: ""

        onRunningChanged: {
            if (!running && sources.currencyWantedPair !== requestPair)
                sources.startCurrencyRequest(null);
        }
        stdout: StdioCollector {
            onStreamFinished: sources.finishCurrency(currencyProcess.requestPair, text)
        }
        stderr: StdioCollector {}
    }

    Process {
        id: fileProcess

        onRunningChanged: {
            if (!running && sources.fileWantedQuery !== sources.fileRequestQuery)
                sources.startFileRequest();
        }
        stdout: StdioCollector {
            onStreamFinished: sources.finishFileRequest(sources.fileRequestQuery, text)
        }
        stderr: StdioCollector {}
    }

    Process {
        id: processProbe
        command: ["ps", "-u", Quickshell.env("USER"), "-o", "pid=,comm=,args="]
        stdout: StdioCollector {
            onStreamFinished: {
                sources.processes = LauncherExtras.parseProcessList(text);
                sources.processesReady = true;
                sources.changed();
            }
        }
        stderr: StdioCollector {}
    }

    Connections {
        target: TimerService
        function onChanged() { sources.changed(); }
    }
}
