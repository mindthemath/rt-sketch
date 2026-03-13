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
    const statLastLen = document.getElementById("stat-last-len");
    const statLastBar = document.getElementById("stat-last-bar");
    const statTotal = document.getElementById("stat-total");

    const sliderK = document.getElementById("slider-k");
    const valK = document.getElementById("val-k");

    const sliderMinLen = document.getElementById("slider-min-len");
    const valMinLen = document.getElementById("val-min-len");
    const sliderMaxLen = document.getElementById("slider-max-len");
    const valMaxLen = document.getElementById("val-max-len");
    const sliderAlpha = document.getElementById("slider-alpha");
    const valAlpha = document.getElementById("val-alpha");
    const sliderGamma = document.getElementById("slider-gamma");
    const valGamma = document.getElementById("val-gamma");

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
            statLastLen.textContent = "-";
            statLastBar.style.width = "0%";
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
        if (msg.last_line_len !== undefined && msg.last_line_len !== null) {
            statLastLen.textContent = msg.last_line_len.toFixed(2);
            const min = parseFloat(sliderMinLen.value);
            const max = parseFloat(sliderMaxLen.value);
            const pct = max > min ? ((msg.last_line_len - min) / (max - min)) * 100 : 50;
            statLastBar.style.width = Math.max(0, Math.min(100, pct)) + "%";
        }
        if (msg.total_length !== undefined && msg.total_length !== null) {
            statTotal.textContent = msg.total_length.toFixed(1);
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

    function togglePlayPause() {
        if (isRunning) {
            send("pause");
        } else {
            send(hasStarted ? "resume" : "start");
        }
    }

    btnToggle.addEventListener("click", togglePlayPause);

    document.addEventListener("keydown", (e) => {
        if (e.code === "Space") {
            e.preventDefault();
            togglePlayPause();
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
        valAlpha.textContent = parseInt(sliderAlpha.value, 10);
        send("set_alpha", parseFloat(sliderAlpha.value));
    });

    sliderGamma.addEventListener("input", () => {
        valGamma.textContent = parseFloat(sliderGamma.value).toFixed(1);
        send("set_gamma", parseFloat(sliderGamma.value));
    });

    // Sampler radio groups
    function setupRadioGroup(groupId, command) {
        const group = document.getElementById(groupId);
        group.addEventListener("change", (e) => {
            if (e.target.type === "radio") {
                group.querySelectorAll("label").forEach(l => l.classList.remove("active"));
                e.target.parentElement.classList.add("active");
                send(command, e.target.value);
            }
        });
    }
    setupRadioGroup("radio-x-sampler", "set_x_sampler");
    setupRadioGroup("radio-y-sampler", "set_y_sampler");
    setupRadioGroup("radio-length-sampler", "set_length_sampler");

    // Target image display size (purely visual, no server command)
    const sliderTargetSize = document.getElementById("slider-target-size");
    const valTargetSize = document.getElementById("val-target-size");
    const targetImgWrapper = document.querySelector(".target-img-wrapper");

    sliderTargetSize.addEventListener("input", () => {
        const pct = sliderTargetSize.value;
        valTargetSize.textContent = pct;
        targetImgWrapper.style.setProperty("--target-size", pct + "%");
    });
})();
