const grid = document.getElementById("grid");
const statusEl = document.getElementById("status");

// Map of instance name -> { container, canvas, ctx, lineCount, widthCm, heightCm, strokeCm }
const instances = new Map();

// Pixels per cm for rendering on the viewer canvas
const PX_PER_CM = 30;

function createInstance(name, widthCm, heightCm, strokeCm) {
    if (instances.has(name)) {
        // Reactivate existing (was disconnected)
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
    label.innerHTML = `<span class="name">${esc(name)}</span><span class="info">0 lines</span><span class="disconnected-badge">disconnected</span>`;
    container.appendChild(label);

    const canvas = document.createElement("canvas");
    container.appendChild(canvas);

    grid.appendChild(container);

    // Remove empty state if present
    const empty = grid.querySelector(".empty-state");
    if (empty) empty.remove();

    const ctx = canvas.getContext("2d");
    const inst = { container, canvas, ctx, label, name, lineCount: 0, widthCm, heightCm, strokeCm };
    resizeCanvas(inst);
    clearCanvas(inst);

    instances.set(name, inst);
    return inst;
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
    inst.lineCount = 0;
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
    inst.lineCount++;
    updateInfo(inst);
}

function updateInfo(inst) {
    const info = inst.label.querySelector(".info");
    info.textContent = inst.lineCount + " lines";
}

function disconnectInstance(name) {
    const inst = instances.get(name);
    if (inst) {
        inst.container.classList.add("disconnected");
    }
}

function esc(s) {
    const d = document.createElement("div");
    d.textContent = s;
    return d.innerHTML;
}

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
                // Replay all instances and their lines
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
                }
                break;

            case "connect":
                createInstance(msg.name, msg.width_cm, msg.height_cm, msg.stroke_width_cm);
                break;

            case "line": {
                let inst = instances.get(msg.name);
                if (!inst) {
                    // Instance connected before we did — create a placeholder
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

            case "disconnect":
                disconnectInstance(msg.name);
                break;
        }
    };
}

// Show empty state initially
grid.innerHTML = '<div class="empty-state">waiting for rt-sketch instances to connect...</div>';

connect();
