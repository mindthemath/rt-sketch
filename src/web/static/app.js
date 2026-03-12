(() => {
    const ws = new WebSocket(`ws://${location.host}/ws`);

    const targetImg = document.getElementById("target-img");
    const canvasImg = document.getElementById("canvas-img");
    const previewImg = document.getElementById("preview-img");

    const statIteration = document.getElementById("stat-iteration");
    const statLines = document.getElementById("stat-lines");
    const statScore = document.getElementById("stat-score");
    const statFps = document.getElementById("stat-fps");

    const sliderK = document.getElementById("slider-k");
    const valK = document.getElementById("val-k");

    // FPS tracking
    let lastUpdateTime = performance.now();
    let frameCount = 0;
    let displayFps = 0;

    function updateFps() {
        frameCount++;
        const now = performance.now();
        const elapsed = now - lastUpdateTime;
        if (elapsed >= 1000) {
            displayFps = (frameCount / (elapsed / 1000)).toFixed(1);
            frameCount = 0;
            lastUpdateTime = now;
        }
        statFps.textContent = displayFps;
    }

    ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);

        if (msg.canvas_png) {
            canvasImg.src = "data:image/png;base64," + msg.canvas_png;
            updateFps();
        }
        if (msg.target_png) {
            targetImg.src = "data:image/png;base64," + msg.target_png;
        }
        if (msg.preview_png) {
            previewImg.src = "data:image/png;base64," + msg.preview_png;
        }
        if (msg.iteration !== undefined && msg.iteration !== null) {
            statIteration.textContent = msg.iteration.toLocaleString();
        }
        if (msg.line_count !== undefined && msg.line_count !== null) {
            statLines.textContent = msg.line_count.toLocaleString();
        }
        if (msg.score !== undefined && msg.score !== null) {
            statScore.textContent = msg.score.toFixed(6);
        }
        if (msg.k !== undefined && msg.k !== null) {
            sliderK.value = msg.k;
            valK.textContent = msg.k;
        }
    };

    ws.onclose = () => {
        console.log("WebSocket closed");
    };

    function send(command, value) {
        if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ command, value }));
        }
    }

    document.getElementById("btn-start").addEventListener("click", () => send("start"));
    document.getElementById("btn-pause").addEventListener("click", () => send("pause"));
    document.getElementById("btn-resume").addEventListener("click", () => send("resume"));
    document.getElementById("btn-reset").addEventListener("click", () => send("reset"));

    sliderK.addEventListener("input", () => {
        valK.textContent = sliderK.value;
        send("set_k", parseInt(sliderK.value, 10));
    });
})();
