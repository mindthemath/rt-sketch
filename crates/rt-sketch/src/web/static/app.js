(() => {
    // --- localStorage persistence ---
    const STORAGE_KEY = "rt-sketch-settings";

    function saveSettings() {
        const settings = {};
        for (const id of ["slider-k", "slider-min-len", "slider-max-len", "slider-alpha", "slider-gamma", "slider-exposure", "slider-contrast", "slider-target-size"]) {
            const el = document.getElementById(id);
            if (el) settings[id] = el.value;
        }
        for (const id of ["radio-x-sampler", "radio-y-sampler", "radio-length-sampler"]) {
            const checked = document.querySelector(`#${id} input[type=radio]:checked`);
            if (checked) settings[id] = checked.value;
        }
        localStorage.setItem(STORAGE_KEY, JSON.stringify(settings));
    }

    function restoreSettings() {
        const raw = localStorage.getItem(STORAGE_KEY);
        if (!raw) return;
        try {
            const settings = JSON.parse(raw);
            for (const id of ["slider-k", "slider-min-len", "slider-max-len", "slider-alpha", "slider-gamma", "slider-exposure", "slider-contrast", "slider-target-size"]) {
                if (settings[id] !== undefined) {
                    const el = document.getElementById(id);
                    if (el) el.value = settings[id];
                }
            }
            for (const id of ["radio-x-sampler", "radio-y-sampler", "radio-length-sampler"]) {
                if (settings[id] !== undefined) {
                    const group = document.getElementById(id);
                    if (!group) continue;
                    const radio = group.querySelector(`input[value="${settings[id]}"]`);
                    if (radio) {
                        radio.checked = true;
                        group.querySelectorAll("label").forEach(l => l.classList.remove("active"));
                        radio.parentElement.classList.add("active");
                    }
                }
            }
        } catch (e) { /* ignore corrupt data */ }
    }

    restoreSettings();

    // DOM refs
    const targetImg = document.getElementById("target-img");
    const canvasImg = document.getElementById("canvas-img");
    const previewImg = document.getElementById("preview-img");
    const bboxOverlay = document.getElementById("bbox-overlay");
    const chkBbox = document.getElementById("chk-bbox");

    // Canvas dimensions in cm (set on init)
    let canvasWidthCm = 0;
    let canvasHeightCm = 0;
    let bboxEnabled = chkBbox.checked;

    chkBbox.addEventListener("change", () => {
        bboxEnabled = chkBbox.checked;
        if (!bboxEnabled) bboxOverlay.style.display = "none";
    });

    const targetPlaceholder = document.getElementById("target-placeholder");
    const canvasPlaceholder = document.getElementById("canvas-placeholder");
    const previewPlaceholder = document.getElementById("preview-placeholder");

    const statIteration = document.getElementById("stat-iteration");
    const statStampsWrap = document.getElementById("stat-stamps-wrap");
    const statStamps = document.getElementById("stat-stamps");
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
    const sliderExposure = document.getElementById("slider-exposure");
    const valExposure = document.getElementById("val-exposure");
    const sliderContrast = document.getElementById("slider-contrast");
    const valContrast = document.getElementById("val-contrast");
    const sliderTargetSize = document.getElementById("slider-target-size");
    const valTargetSize = document.getElementById("val-target-size");
    const targetImgWrapper = document.querySelector(".target-img-wrapper");

    // Sync display labels with (possibly restored) slider values
    valK.textContent = sliderK.value;
    valMinLen.textContent = parseFloat(sliderMinLen.value).toFixed(1);
    valMaxLen.textContent = parseFloat(sliderMaxLen.value).toFixed(1);
    valAlpha.textContent = parseInt(sliderAlpha.value, 10);
    valGamma.textContent = parseFloat(sliderGamma.value).toFixed(1);
    valExposure.textContent = parseFloat(sliderExposure.value).toFixed(1);
    valContrast.textContent = parseFloat(sliderContrast.value).toFixed(1);
    valTargetSize.textContent = sliderTargetSize.value;
    targetImgWrapper.style.setProperty("--target-size", sliderTargetSize.value + "%");

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
    let pausedReason = null; // set when auto-paused by a limit
    const btnToggle = document.getElementById("btn-toggle");
    const btnContinue = document.getElementById("btn-continue");

    function updateToggleButton() {
        if (pausedReason) {
            btnToggle.textContent = "Resume";
            btnContinue.style.display = "";
        } else {
            btnToggle.textContent = isRunning ? "Pause" : (hasStarted ? "Resume" : "Start");
            btnContinue.style.display = "none";
        }
    }

    // --- WebSocket ---
    const ws = new WebSocket(`ws://${location.host}/ws`);

    function send(command, value) {
        if (ws.readyState === WebSocket.OPEN) {
            ws.send(JSON.stringify({ command, value }));
        }
    }

    // On connect, push all restored settings to the server
    ws.onopen = () => {
        send("set_k", parseInt(sliderK.value, 10));
        send("set_min_len", parseFloat(sliderMinLen.value));
        send("set_max_len", parseFloat(sliderMaxLen.value));
        send("set_alpha", parseFloat(sliderAlpha.value));
        send("set_gamma", parseFloat(sliderGamma.value));
        send("set_exposure", parseFloat(sliderExposure.value));
        send("set_contrast", parseFloat(sliderContrast.value));

        for (const [groupId, cmd] of [["radio-x-sampler", "set_x_sampler"], ["radio-y-sampler", "set_y_sampler"], ["radio-length-sampler", "set_length_sampler"]]) {
            const checked = document.querySelector(`#${groupId} input[type=radio]:checked`);
            if (checked) send(cmd, checked.value);
        }
    };

    ws.onmessage = (event) => {
        const msg = JSON.parse(event.data);

        if (msg.canvas_width_cm) canvasWidthCm = msg.canvas_width_cm;
        if (msg.canvas_height_cm) canvasHeightCm = msg.canvas_height_cm;

        if (msg.paused_reason) {
            pausedReason = msg.paused_reason;
        }
        if (msg.running !== undefined && msg.running !== null) {
            isRunning = msg.running;
            if (isRunning) {
                hasStarted = true;
                pausedReason = null;
            }
            updateToggleButton();
        }

        if (msg.type === "reset") {
            hideImg(canvasImg, canvasPlaceholder);
            hideImg(previewImg, previewPlaceholder);
            bboxOverlay.style.display = "none";
            statFps.textContent = "-";
            statLastLen.textContent = "-";
            statLastBar.style.width = "0%";
            hasStarted = false;
            pausedReason = null;
            updateToggleButton();
        }

        if (msg.canvas_png) {
            canvasImg.src = "data:image/png;base64," + msg.canvas_png;
            showImg(canvasImg, canvasPlaceholder);
            updateFps();
        }
        // Bbox overlay
        if (bboxEnabled && msg.last_bbox && canvasWidthCm > 0 && canvasHeightCm > 0) {
            const [minX, minY, maxX, maxY] = msg.last_bbox;
            const imgW = canvasImg.clientWidth;
            const imgH = canvasImg.clientHeight;
            const sx = imgW / canvasWidthCm;
            const sy = imgH / canvasHeightCm;
            bboxOverlay.style.left = (minX * sx) + "px";
            bboxOverlay.style.top = (minY * sy) + "px";
            bboxOverlay.style.width = ((maxX - minX) * sx) + "px";
            bboxOverlay.style.height = ((maxY - minY) * sy) + "px";
            bboxOverlay.style.display = "block";
        } else if (!msg.last_bbox) {
            bboxOverlay.style.display = "none";
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
        if (msg.stamp_count !== undefined && msg.stamp_count !== null) {
            statStampsWrap.style.display = "";
            statStamps.textContent = msg.stamp_count.toLocaleString();
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

    // --- Controls ---
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
    btnContinue.addEventListener("click", () => {
        send("continue");
    });
    document.getElementById("btn-reset").addEventListener("click", () => send("reset"));
    document.getElementById("btn-export").addEventListener("click", () => {
        window.open("/svg", "_blank");
    });

    sliderK.addEventListener("input", () => {
        valK.textContent = sliderK.value;
        send("set_k", parseInt(sliderK.value, 10));
        saveSettings();
    });

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
        saveSettings();
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
        saveSettings();
    });

    sliderAlpha.addEventListener("input", () => {
        valAlpha.textContent = parseInt(sliderAlpha.value, 10);
        send("set_alpha", parseFloat(sliderAlpha.value));
        saveSettings();
    });

    sliderGamma.addEventListener("input", () => {
        valGamma.textContent = parseFloat(sliderGamma.value).toFixed(1);
        send("set_gamma", parseFloat(sliderGamma.value));
        saveSettings();
    });

    sliderExposure.addEventListener("input", () => {
        valExposure.textContent = parseFloat(sliderExposure.value).toFixed(1);
        send("set_exposure", parseFloat(sliderExposure.value));
        saveSettings();
    });

    sliderContrast.addEventListener("input", () => {
        valContrast.textContent = parseFloat(sliderContrast.value).toFixed(1);
        send("set_contrast", parseFloat(sliderContrast.value));
        saveSettings();
    });

    // Sampler radio groups
    function setupRadioGroup(groupId, command) {
        const group = document.getElementById(groupId);
        group.addEventListener("change", (e) => {
            if (e.target.type === "radio") {
                group.querySelectorAll("label").forEach(l => l.classList.remove("active"));
                e.target.parentElement.classList.add("active");
                send(command, e.target.value);
                saveSettings();
            }
        });
    }
    setupRadioGroup("radio-x-sampler", "set_x_sampler");
    setupRadioGroup("radio-y-sampler", "set_y_sampler");
    setupRadioGroup("radio-length-sampler", "set_length_sampler");

    sliderTargetSize.addEventListener("input", () => {
        const pct = sliderTargetSize.value;
        valTargetSize.textContent = pct;
        targetImgWrapper.style.setProperty("--target-size", pct + "%");
        saveSettings();
    });
})();
