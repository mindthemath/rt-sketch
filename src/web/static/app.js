(() => {
    const ws = new WebSocket(`ws://${location.host}/ws`);

    const targetImg = document.getElementById("target-img");
    const canvasImg = document.getElementById("canvas-img");
    const previewImg = document.getElementById("preview-img");

    const targetPlaceholder = document.getElementById("target-placeholder");
    const canvasPlaceholder = document.getElementById("canvas-placeholder");
    const previewPlaceholder = document.getElementById("preview-placeholder");

    const statIteration = document.getElementById("stat-iteration");
    const statLines = document.getElementById("stat-lines");
    const statScore = document.getElementById("stat-score");
    const statFps = document.getElementById("stat-fps");

    const sliderK = document.getElementById("slider-k");
    const valK = document.getElementById("val-k");

    const sliderMinLen = document.getElementById("slider-min-len");
    const valMinLen = document.getElementById("val-min-len");
    const sliderMaxLen = document.getElementById("slider-max-len");
    const valMaxLen = document.getElementById("val-max-len");
    const sliderAlpha = document.getElementById("slider-alpha");
    const valAlpha = document.getElementById("val-alpha");

    function showImg(img, placeholder) {
        img.style.display = "block";
        placeholder.style.display = "none";
    }

    function hideImg(img, placeholder) {
        img.style.display = "none";
        img.removeAttribute("src");
        placeholder.style.display = "flex";
    }

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

    let isRunning = false;
    let hasStarted = false;
    const btnToggle = document.getElementById("btn-toggle");

    function updateToggleButton() {
        btnToggle.textContent = isRunning ? "Pause" : (hasStarted ? "Resume" : "Start");
    }

    ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);

        if (msg.running !== undefined && msg.running !== null) {
            isRunning = msg.running;
            if (isRunning) hasStarted = true;
            updateToggleButton();
        }

        if (msg.type === "reset") {
            hideImg(canvasImg, canvasPlaceholder);
            hideImg(previewImg, previewPlaceholder);
            statFps.textContent = "-";
            hasStarted = false;
            updateToggleButton();
        }

        if (msg.canvas_png) {
            canvasImg.src = "data:image/png;base64," + msg.canvas_png;
            showImg(canvasImg, canvasPlaceholder);
            updateFps();
        }
        if (msg.target_png) {
            targetImg.src = "data:image/png;base64," + msg.target_png;
            showImg(targetImg, targetPlaceholder);
        }
        if (msg.preview_png) {
            previewImg.src = "data:image/png;base64," + msg.preview_png;
            showImg(previewImg, previewPlaceholder);
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

    btnToggle.addEventListener("click", () => {
        if (isRunning) {
            send("pause");
        } else {
            send(hasStarted ? "resume" : "start");
        }
    });
    document.getElementById("btn-reset").addEventListener("click", () => send("reset"));
    document.getElementById("btn-export").addEventListener("click", () => {
        window.open("/svg", "_blank");
    });

    sliderK.addEventListener("input", () => {
        valK.textContent = sliderK.value;
        send("set_k", parseInt(sliderK.value, 10));
    });

    // Line length sliders — clamp min <= max
    sliderMinLen.addEventListener("input", () => {
        let min = parseFloat(sliderMinLen.value);
        let max = parseFloat(sliderMaxLen.value);
        if (min > max) {
            sliderMaxLen.value = min;
            valMaxLen.textContent = min.toFixed(1);
            send("set_max_len", min);
        }
        valMinLen.textContent = min.toFixed(1);
        send("set_min_len", min);
    });

    sliderMaxLen.addEventListener("input", () => {
        let max = parseFloat(sliderMaxLen.value);
        let min = parseFloat(sliderMinLen.value);
        if (max < min) {
            sliderMinLen.value = max;
            valMinLen.textContent = max.toFixed(1);
            send("set_min_len", max);
        }
        valMaxLen.textContent = max.toFixed(1);
        send("set_max_len", max);
    });

    sliderAlpha.addEventListener("input", () => {
        valAlpha.textContent = parseFloat(sliderAlpha.value).toFixed(1);
        send("set_alpha", parseFloat(sliderAlpha.value));
    });
})();
