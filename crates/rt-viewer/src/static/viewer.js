const grid = document.getElementById("grid");
const statusEl = document.getElementById("status");
const controlsEl = document.getElementById("controls");
const btnToggle = document.getElementById("btn-toggle");
const btnReset = document.getElementById("btn-reset");
const btnExport = document.getElementById("btn-export");
const btnTidy = document.getElementById("btn-tidy");
const totalLinesEl = document.getElementById("total-lines");

// Map of instance name -> { container, canvas, ctx, lineCount, widthCm, heightCm, strokeCm, running }
const instances = new Map();

// Pixels per cm for rendering on the viewer canvas
const PX_PER_CM = 30;

let readOnly = true;
let globalRunning = false;
let globalLineCount = 0;
let globalLengthCm = 0;

function createInstance(name, widthCm, heightCm, strokeCm) {
    if (instances.has(name)) {
        const inst = instances.get(name);
        inst.container.classList.remove("disconnected");
        inst.widthCm = widthCm;
        inst.heightCm = heightCm;
        inst.strokeCm = strokeCm;
        resizeCanvas(inst);
        clearCanvas(inst);
        return inst;
    }

    const container = document.createElement("div");
    container.className = "instance";

    const label = document.createElement("div");
    label.className = "label";

    const nameSpan = document.createElement("span");
    nameSpan.className = "name";
    nameSpan.textContent = name;
    label.appendChild(nameSpan);

    const infoSpan = document.createElement("span");
    infoSpan.className = "info";
    infoSpan.textContent = "0 L";
    label.appendChild(infoSpan);

    const disconnBadge = document.createElement("span");
    disconnBadge.className = "disconnected-badge";
    disconnBadge.textContent = "disconnected";
    label.appendChild(disconnBadge);

    // Per-instance controls (inline in label row)
    const ctrlBar = document.createElement("span");
    ctrlBar.className = "instance-controls hidden";

    const bToggle = document.createElement("button");
    bToggle.textContent = "\u25B6";
    bToggle.title = "Start/Pause";
    bToggle.addEventListener("click", () => {
        const inst = instances.get(name);
        if (inst.running) {
            sendCommand("pause", name);
            setInstanceRunning(name, false);
        } else {
            sendCommand("play", name);
            setInstanceRunning(name, true);
        }
        updateGlobalToggle();
    });

    const bReset = document.createElement("button");
    bReset.textContent = "\u21BA";
    bReset.title = "Reset";
    bReset.addEventListener("click", () => {
        sendCommand("reset", name);
    });

    const bExport = document.createElement("button");
    bExport.textContent = "Save";
    bExport.title = "Save SVG";
    bExport.addEventListener("click", () => {
        const inst = instances.get(name);
        const ts = new Date().toISOString().replace(/[:.]/g, "-");
        exportSvg(inst, name + "_" + ts + ".svg");
    });

    ctrlBar.appendChild(bToggle);
    ctrlBar.appendChild(bReset);
    ctrlBar.appendChild(bExport);
    label.appendChild(ctrlBar);

    container.appendChild(label);

    const canvas = document.createElement("canvas");
    container.appendChild(canvas);

    grid.appendChild(container);

    const empty = grid.querySelector(".empty-state");
    if (empty) empty.remove();

    const ctx = canvas.getContext("2d");
    const inst = { container, canvas, ctx, label, ctrlBar, toggleBtn: bToggle, name, lineCount: 0, totalLengthCm: 0, widthCm, heightCm, strokeCm, running: false, lines: [] };
    resizeCanvas(inst);
    clearCanvas(inst);

    if (!readOnly) {
        ctrlBar.classList.remove("hidden");
    }

    instances.set(name, inst);
    return inst;
}

function setInstanceRunning(name, running) {
    const inst = instances.get(name);
    if (!inst) return;
    inst.running = running;
    inst.toggleBtn.textContent = running ? "\u23F8" : "\u25B6";
    inst.toggleBtn.classList.toggle("playing", running);
}

function resizeCanvas(inst) {
    const w = Math.round(inst.widthCm * PX_PER_CM);
    const h = Math.round(inst.heightCm * PX_PER_CM);
    inst.canvas.width = w;
    inst.canvas.height = h;
}

function clearCanvas(inst) {
    inst.ctx.fillStyle = "#fff";
    inst.ctx.fillRect(0, 0, inst.canvas.width, inst.canvas.height);
    globalLineCount -= inst.lineCount;
    globalLengthCm -= inst.totalLengthCm;
    inst.lineCount = 0;
    inst.totalLengthCm = 0;
    inst.lines = [];
    updateInfo(inst);
}

function drawLine(inst, x1, y1, x2, y2, width) {
    const ctx = inst.ctx;
    const s = PX_PER_CM;
    ctx.strokeStyle = "#000";
    ctx.lineCap = "round";
    ctx.lineWidth = width * s;
    ctx.beginPath();
    ctx.moveTo(x1 * s, y1 * s);
    ctx.lineTo(x2 * s, y2 * s);
    ctx.stroke();
    const dx = x2 - x1;
    const dy = y2 - y1;
    const segLen = Math.sqrt(dx * dx + dy * dy);
    inst.totalLengthCm += segLen;
    globalLengthCm += segLen;
    inst.lineCount++;
    globalLineCount++;
    inst.lines.push({ x1, y1, x2, y2, width });
    updateInfo(inst);
}

function formatLength(cm) {
    if (cm >= 100) return (cm / 100).toFixed(1) + " m";
    return cm.toFixed(1) + " cm";
}

function updateInfo(inst) {
    const info = inst.label.querySelector(".info");
    info.textContent = inst.lineCount + " L · " + formatLength(inst.totalLengthCm);
    updateTotalLines();
}

function updateTotalLines() {
    totalLinesEl.textContent = globalLineCount + " L · " + formatLength(globalLengthCm);
}

function disconnectInstance(name) {
    const inst = instances.get(name);
    if (inst) {
        inst.container.classList.add("disconnected");
    }
}

function tidyDisconnected() {
    for (const [name, inst] of instances) {
        if (inst.container.classList.contains("disconnected")) {
            globalLineCount -= inst.lineCount;
            globalLengthCm -= inst.totalLengthCm;
            inst.container.remove();
            instances.delete(name);
        }
    }
    updateTotalLines();
    if (instances.size === 0) {
        grid.innerHTML = '<div class="empty-state">waiting for rt-sketch instances to connect...</div>';
    }
}

function esc(s) {
    const d = document.createElement("div");
    d.textContent = s;
    return d.innerHTML;
}

function sendCommand(type, name) {
    if (ws && ws.readyState === WebSocket.OPEN) {
        const msg = { type };
        if (name) msg.name = name;
        ws.send(JSON.stringify(msg));
    }
}

function updateGlobalToggle() {
    // Global is "running" if any instance is running
    globalRunning = [...instances.values()].some(i => i.running);
    btnToggle.textContent = globalRunning ? "Pause All" : "Play All";
}

// --- Export ---

function buildSvg(inst) {
    const w = inst.widthCm;
    const h = inst.heightCm;
    // Use mm for SVG units (1cm = 10mm)
    const lines = inst.lines.map(l =>
        `<line x1="${l.x1}" y1="${l.y1}" x2="${l.x2}" y2="${l.y2}" stroke="black" stroke-width="${l.width}" stroke-linecap="round"/>`
    ).join("\n  ");
    return `<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="${w}cm" height="${h}cm" viewBox="0 0 ${w} ${h}">
  <rect width="${w}" height="${h}" fill="white"/>
  ${lines}
</svg>`;
}

function exportSvg(inst, filename) {
    const svg = buildSvg(inst);
    const blob = new Blob([svg], { type: "image/svg+xml" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = filename;
    a.click();
    URL.revokeObjectURL(url);
}

function buildZip(files) {
    // Minimal ZIP (STORE, no compression) — works everywhere, no dependencies
    const enc = new TextEncoder();
    const localHeaders = [];
    const centralEntries = [];
    let offset = 0;

    for (const { name, data } of files) {
        const nameBytes = enc.encode(name);
        const dataBytes = typeof data === "string" ? enc.encode(data) : data;
        const crc = crc32(dataBytes);

        // Local file header (30 + name + data)
        const local = new Uint8Array(30 + nameBytes.length + dataBytes.length);
        const lv = new DataView(local.buffer);
        lv.setUint32(0, 0x04034b50, true);   // signature
        lv.setUint16(4, 20, true);            // version needed
        lv.setUint16(6, 0, true);             // flags
        lv.setUint16(8, 0, true);             // compression: STORE
        lv.setUint16(10, 0, true);            // mod time
        lv.setUint16(12, 0, true);            // mod date
        lv.setUint32(14, crc, true);
        lv.setUint32(18, dataBytes.length, true); // compressed
        lv.setUint32(22, dataBytes.length, true); // uncompressed
        lv.setUint16(26, nameBytes.length, true);
        lv.setUint16(28, 0, true);            // extra length
        local.set(nameBytes, 30);
        local.set(dataBytes, 30 + nameBytes.length);
        localHeaders.push(local);

        // Central directory entry (46 + name)
        const central = new Uint8Array(46 + nameBytes.length);
        const cv = new DataView(central.buffer);
        cv.setUint32(0, 0x02014b50, true);   // signature
        cv.setUint16(4, 20, true);            // version made by
        cv.setUint16(6, 20, true);            // version needed
        cv.setUint16(8, 0, true);             // flags
        cv.setUint16(10, 0, true);            // compression: STORE
        cv.setUint16(12, 0, true);            // mod time
        cv.setUint16(14, 0, true);            // mod date
        cv.setUint32(16, crc, true);
        cv.setUint32(20, dataBytes.length, true);
        cv.setUint32(24, dataBytes.length, true);
        cv.setUint16(28, nameBytes.length, true);
        cv.setUint16(30, 0, true);            // extra length
        cv.setUint16(32, 0, true);            // comment length
        cv.setUint16(34, 0, true);            // disk number
        cv.setUint16(36, 0, true);            // internal attrs
        cv.setUint32(38, 0, true);            // external attrs
        cv.setUint32(42, offset, true);       // local header offset
        central.set(nameBytes, 46);
        centralEntries.push(central);

        offset += local.length;
    }

    const centralSize = centralEntries.reduce((s, e) => s + e.length, 0);

    // End of central directory (22 bytes)
    const eocd = new Uint8Array(22);
    const ev = new DataView(eocd.buffer);
    ev.setUint32(0, 0x06054b50, true);
    ev.setUint16(4, 0, true);
    ev.setUint16(6, 0, true);
    ev.setUint16(8, files.length, true);
    ev.setUint16(10, files.length, true);
    ev.setUint32(12, centralSize, true);
    ev.setUint32(16, offset, true);
    ev.setUint16(20, 0, true);

    const blob = new Blob([...localHeaders, ...centralEntries, eocd], { type: "application/zip" });
    return blob;
}

function crc32(bytes) {
    let crc = 0xFFFFFFFF;
    for (let i = 0; i < bytes.length; i++) {
        crc ^= bytes[i];
        for (let j = 0; j < 8; j++) {
            crc = (crc >>> 1) ^ (crc & 1 ? 0xEDB88320 : 0);
        }
    }
    return (crc ^ 0xFFFFFFFF) >>> 0;
}

function exportAll() {
    const now = new Date();
    const ts = now.toISOString().replace(/[:.]/g, "-").replace("Z", "");
    const folder = "rt-viewer-export-" + ts;

    const files = [];
    for (const [name, inst] of instances) {
        files.push({ name: folder + "/" + name + ".svg", data: buildSvg(inst) });
    }

    const blob = buildZip(files);
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = folder + ".zip";
    a.click();
    URL.revokeObjectURL(url);
}

// --- Global controls ---

btnToggle.addEventListener("click", () => {
    if (globalRunning) {
        sendCommand("pause");
        for (const [name] of instances) {
            setInstanceRunning(name, false);
        }
        globalRunning = false;
    } else {
        sendCommand("play");
        for (const [name] of instances) {
            setInstanceRunning(name, true);
        }
        globalRunning = true;
    }
    btnToggle.textContent = globalRunning ? "Pause All" : "Play All";
});

btnReset.addEventListener("click", () => {
    sendCommand("reset");
});

btnExport.addEventListener("click", () => {
    exportAll();
});

btnTidy.addEventListener("click", () => {
    tidyDisconnected();
});

// --- WebSocket ---

let ws;
let reconnectDelay = 1000;

function connect() {
    ws = new WebSocket("ws://" + location.host + "/ws");

    ws.onopen = function () {
        statusEl.textContent = "connected";
        statusEl.className = "status connected";
        reconnectDelay = 1000;
    };

    ws.onclose = function () {
        statusEl.textContent = "disconnected";
        statusEl.className = "status disconnected";
        setTimeout(connect, reconnectDelay);
        reconnectDelay = Math.min(reconnectDelay * 2, 10000);
    };

    ws.onmessage = function (e) {
        const msg = JSON.parse(e.data);

        switch (msg.type) {
            case "init":
                readOnly = msg.read_only;
                if (!readOnly) {
                    controlsEl.classList.remove("hidden");
                }
                for (const instData of msg.instances) {
                    const inst = createInstance(
                        instData.name,
                        instData.width_cm,
                        instData.height_cm,
                        instData.stroke_width_cm
                    );
                    for (const l of instData.lines) {
                        drawLine(inst, l.x1, l.y1, l.x2, l.y2, l.width);
                    }
                    setInstanceRunning(inst.name, !instData.paused);
                }
                updateGlobalToggle();
                break;

            case "connect":
                createInstance(msg.name, msg.width_cm, msg.height_cm, msg.stroke_width_cm);
                updateGlobalToggle();
                break;

            case "line": {
                let inst = instances.get(msg.name);
                if (!inst) {
                    inst = createInstance(msg.name, 10, 10, 0.05);
                }
                drawLine(inst, msg.x1, msg.y1, msg.x2, msg.y2, msg.width);
                break;
            }

            case "reset": {
                const inst = instances.get(msg.name);
                if (inst) clearCanvas(inst);
                break;
            }

            case "state":
                setInstanceRunning(msg.name, !msg.paused);
                updateGlobalToggle();
                break;

            case "disconnect":
                disconnectInstance(msg.name);
                updateGlobalToggle();
                break;
        }
    };
}

grid.innerHTML = '<div class="empty-state">waiting for rt-sketch instances to connect...</div>';

connect();
