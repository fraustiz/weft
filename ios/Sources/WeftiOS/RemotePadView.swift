// Remote Control sheet (`docs/features.md` §8): multi-touch Touchpad, live
// Keyboard with special keycaps and modifiers, and Presentation remote with
// CoreMotion gyroscope laser pointer.

#if canImport(UIKit)
import CoreMotion
import SwiftUI
import UIKit
import WeftMobileKit

private enum RemoteTab: String, CaseIterable {
    case trackpad
    case keyboard
    case presenter
}

struct RemotePadView: View {
    @ObservedObject var model: MobileAppModel
    let peer: PeerSnapshot

    @StateObject private var selectedTab = Local<RemoteTab>(.trackpad)
    @StateObject private var touchPoint = Local<CGPoint?>(nil)
    @StateObject private var speedDraft = Local<Double>(MobileStore.defaultPointerSpeed)
    @StateObject private var typedBuffer = Local("")
    @StateObject private var ctrlHeld = Local(false)
    @StateObject private var altHeld = Local(false)
    @StateObject private var shiftHeld = Local(false)
    @StateObject private var metaHeld = Local(false)
    @StateObject private var isPresenting = Local(false)
    @StateObject private var laserController = LaserPointerController()
    @FocusState private var keyboardFocused: Bool
    @Environment(\.dismiss) private var dismiss

    private let inputQueue = RemoteInputQueue()

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                Picker(t("Mode", "Mode"), selection: $selectedTab.value) {
                    Text(t("Trackpad", "Pavé")).tag(RemoteTab.trackpad)
                    Text(t("Keyboard", "Clavier")).tag(RemoteTab.keyboard)
                    Text(t("Presenter", "Présentation")).tag(RemoteTab.presenter)
                }
                .pickerStyle(.segmented)

                switch selectedTab.value {
                case .trackpad:
                    trackpadContent
                case .keyboard:
                    keyboardContent
                case .presenter:
                    presenterContent
                }
            }
            .padding(16)
            .background(Color(uiColor: .systemGroupedBackground).ignoresSafeArea())
            .navigationTitle(peer.displayName)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(t("Done", "Terminé")) {
                        dismiss()
                    }
                }
            }
            .onAppear {
                speedDraft.value = model.snapshot?.pointerSpeed ?? MobileStore.defaultPointerSpeed
                UIApplication.shared.isIdleTimerDisabled = true
            }
            .onDisappear {
                laserController.stopPointing(queue: inputQueue)
                flushQueueNow()
                UIApplication.shared.isIdleTimerDisabled = false
            }
            .task {
                // 60 Hz packet flush loop while sheet is open.
                while !Task.isCancelled {
                    let packets = inputQueue.drain()
                    if !packets.isEmpty {
                        await model.session?.sendRemotePackets(packets, to: peer.id)
                    }
                    try? await Task.sleep(nanoseconds: 16_000_000)
                }
            }
        }
    }

    private func flushQueueNow() {
        let packets = inputQueue.drain()
        guard !packets.isEmpty else { return }
        Task {
            await model.session?.sendRemotePackets(packets, to: peer.id)
        }
    }

    // MARK: - Trackpad

    private var trackpadContent: some View {
        VStack(spacing: 12) {
            ZStack(alignment: .topLeading) {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(Color(uiColor: .secondarySystemGroupedBackground))
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)

                if touchPoint.value == nil {
                    Text(t(
                        "Drag to move · Tap to click · Two fingers to scroll or right-click · Hold to drag",
                        "Glisser : déplacer · Toucher : clic · Deux doigts : défiler ou clic droit · Maintenir : glisser"
                    ))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(24)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .allowsHitTesting(false)
                }

                MultiTouchPadSurface(
                    sensitivity: speedDraft.value,
                    queue: inputQueue,
                    onTouchPoint: { point in
                        touchPoint.value = point
                    }
                )
                .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))

                if let pt = touchPoint.value {
                    RoundedRectangle(cornerRadius: 3, style: .continuous)
                        .fill(Color.weftOrange)
                        .frame(width: 12, height: 12)
                        .position(x: pt.x, y: pt.y)
                        .allowsHitTesting(false)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            HStack(spacing: 8) {
                mouseButton(t("Left", "Gauche"), button: InputClickPayload.buttonLeft)
                mouseButton(t("Middle", "Milieu"), button: InputClickPayload.buttonMiddle, compact: true)
                mouseButton(t("Right", "Droit"), button: InputClickPayload.buttonRight)
            }

            HStack(spacing: 12) {
                Text(t("Speed", "Vitesse"))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Slider(
                    value: $speedDraft.value,
                    in: MobileStore.minPointerSpeed...MobileStore.maxPointerSpeed
                ) { editing in
                    if !editing {
                        model.session?.updatePointerSpeed(speedDraft.value)
                    }
                }
            }
            .padding(.top, 4)
        }
    }

    private func mouseButton(_ label: String, button: String, compact: Bool = false) -> some View {
        Button {
            UIImpactFeedbackGenerator(style: .rigid).impactOccurred()
            inputQueue.add(.inputClick(InputClickPayload(button: button)))
        } label: {
            Text(label)
                .font(.subheadline.weight(.medium))
                .foregroundStyle(.primary)
                .frame(maxWidth: compact ? 92 : .infinity, minHeight: 52)
                .background(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(Color(uiColor: .secondarySystemGroupedBackground))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
    }

    // MARK: - Live Keyboard

    private var keyboardContent: some View {
        VStack(alignment: .leading, spacing: 14) {
            VStack(alignment: .leading, spacing: 6) {
                Text(t("Live Input", "Saisie en direct"))
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    TextField(
                        t("Type on computer", "Saisir sur l’ordinateur"),
                        text: $typedBuffer.value,
                        prompt: Text(Language.current == .french
                            ? "Écrire sur \(peer.displayName)…"
                            : "Type on \(peer.displayName)…")
                    )
                    .focused($keyboardFocused)
                    .autocorrectionDisabled()
                    .textInputAutocapitalization(.never)
                    .submitLabel(.send)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 10)
                    .background(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .fill(Color(uiColor: .secondarySystemGroupedBackground))
                    )
                    .onSubmit {
                        sendNamedKey("enter")
                        typedBuffer.value = ""
                    }
                    .onChange(of: typedBuffer.value) { oldValue, newValue in
                        let diff = RemoteInputMath.typedChange(old: oldValue, new: newValue)
                        if diff.backspaces > 0 {
                            for _ in 0..<diff.backspaces {
                                inputQueue.add(.inputKey(InputKeyPayload(key: "backspace")))
                            }
                        }
                        if !diff.inserted.isEmpty {
                            inputQueue.add(.inputKey(makeKeyPayload(text: diff.inserted, key: nil)))
                            clearOneShotModifiers()
                        }
                    }

                    Button(t("Clear", "Effacer")) {
                        typedBuffer.value = ""
                    }
                    .disabled(typedBuffer.value.isEmpty)
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text(t("Modifiers", "Modificateurs"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                HStack(spacing: 8) {
                    modifierCap("Ctrl", active: $ctrlHeld.value)
                    modifierCap("Alt", active: $altHeld.value)
                    modifierCap("Shift", active: $shiftHeld.value)
                    modifierCap(peer.platform == "macos" ? "Cmd" : "Meta", active: $metaHeld.value)
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text(t("Keys", "Touches"))
                    .font(.caption)
                    .foregroundStyle(.secondary)
                LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 8), count: 4), spacing: 8) {
                    keyCap("Esc", key: "escape")
                    keyCap("Tab", key: "tab")
                    keyCap("Enter", key: "enter")
                    keyCap("⌫", key: "backspace")
                    keyCap("←", key: "left")
                    keyCap("↑", key: "up")
                    keyCap("↓", key: "down")
                    keyCap("→", key: "right")
                    keyCap("Home", key: "home")
                    keyCap("End", key: "end")
                    keyCap("PgUp", key: "pageUp")
                    keyCap("PgDn", key: "pageDown")
                }
            }

            Spacer(minLength: 0)
        }
        .onAppear {
            keyboardFocused = true
        }
    }

    private func makeKeyPayload(text: String?, key: String?) -> InputKeyPayload {
        InputKeyPayload(
            text: text,
            key: key,
            ctrl: ctrlHeld.value,
            alt: altHeld.value,
            shift: shiftHeld.value,
            meta: metaHeld.value
        )
    }

    private func clearOneShotModifiers() {
        ctrlHeld.value = false
        altHeld.value = false
        shiftHeld.value = false
        metaHeld.value = false
    }

    private func sendNamedKey(_ key: String) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        inputQueue.add(.inputKey(makeKeyPayload(text: nil, key: key)))
        clearOneShotModifiers()
    }

    private func modifierCap(_ title: String, active: Binding<Bool>) -> some View {
        Button {
            active.wrappedValue.toggle()
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
        } label: {
            Text(title)
                .font(.footnote.monospaced().weight(.medium))
                .foregroundStyle(active.wrappedValue ? Color.weftInk : Color.primary)
                .frame(maxWidth: .infinity, minHeight: 40)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(active.wrappedValue ? Color.weftOrange : Color(uiColor: .secondarySystemGroupedBackground))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
    }

    private func keyCap(_ label: String, key: String) -> some View {
        Button {
            sendNamedKey(key)
        } label: {
            Text(label)
                .font(.footnote.monospaced().weight(.medium))
                .foregroundStyle(.primary)
                .frame(maxWidth: .infinity, minHeight: 46)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(Color(uiColor: .secondarySystemGroupedBackground))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
                )
        }
        .buttonStyle(.plain)
    }

    // MARK: - Presenter & Gyroscope Laser Pointer

    private var presenterContent: some View {
        VStack(spacing: 12) {
            Button {
                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                inputQueue.add(.presenter(PresenterPayload(action: "next")))
            } label: {
                VStack(spacing: 6) {
                    Image(systemName: "arrow.right")
                        .font(.system(size: 24, weight: .medium))
                    Text(t("Next Slide", "Diapositive suivante"))
                        .font(.title3.weight(.medium))
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .buttonStyle(WeftProminentButtonStyle(fullWidth: true, large: true))

            Button {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                inputQueue.add(.presenter(PresenterPayload(action: "previous")))
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: "arrow.left")
                    Text(t("Previous Slide", "Diapositive précédente"))
                        .font(.headline)
                }
                .foregroundStyle(.primary)
                .frame(maxWidth: .infinity, minHeight: 76)
                .background(
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .fill(Color(uiColor: .secondarySystemGroupedBackground))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 14, style: .continuous)
                        .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
                )
            }
            .buttonStyle(.plain)

            HStack(spacing: 10) {
                if laserController.isAvailable {
                    HoldLaserButton(
                        pointing: laserController.isPointing,
                        onPressChanged: { pressed in
                            if pressed {
                                UIImpactFeedbackGenerator(style: .rigid).impactOccurred()
                                laserController.startPointing(queue: inputQueue)
                            } else {
                                laserController.stopPointing(queue: inputQueue)
                            }
                        }
                    )
                }

                Button {
                    isPresenting.value.toggle()
                    let action = isPresenting.value ? "start" : "stop"
                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    inputQueue.add(.presenter(PresenterPayload(action: action)))
                } label: {
                    Text(isPresenting.value
                         ? t("End Slideshow", "Arrêter le diaporama")
                         : t("Start Slideshow", "Lancer le diaporama"))
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(.primary)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal, 12)
                        .frame(maxWidth: .infinity, minHeight: 60)
                        .background(
                            RoundedRectangle(cornerRadius: 14, style: .continuous)
                                .fill(Color(uiColor: .secondarySystemGroupedBackground))
                        )
                        .overlay(
                            RoundedRectangle(cornerRadius: 14, style: .continuous)
                                .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
                        )
                }
                .buttonStyle(.plain)
            }
        }
    }
}

// MARK: - Hold-to-Point Laser Button

private struct HoldLaserButton: View {
    let pointing: Bool
    let onPressChanged: (Bool) -> Void
    @StateObject private var pressing = Local(false)

    var body: some View {
        Text(pointing ? t("Pointing…", "Pointeur actif…") : t("Hold to Point", "Maintenir pour pointer"))
            .font(.subheadline.weight(.medium))
            .foregroundStyle(pointing ? Color.weftInk : Color.primary)
            .multilineTextAlignment(.center)
            .padding(.horizontal, 12)
            .frame(maxWidth: .infinity, minHeight: 60)
            .background(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .fill(pointing ? Color.weftOrange : Color(uiColor: .secondarySystemGroupedBackground))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 14, style: .continuous)
                    .strokeBorder(Color.primary.opacity(0.08), lineWidth: 1)
            )
            .gesture(
                DragGesture(minimumDistance: 0)
                    .onChanged { _ in
                        if !pressing.value {
                            pressing.value = true
                            onPressChanged(true)
                        }
                    }
                    .onEnded { _ in
                        if pressing.value {
                            pressing.value = false
                            onPressChanged(false)
                        }
                    }
            )
            .accessibilityAddTraits(.isButton)
    }
}

// MARK: - CoreMotion Attitude Laser Controller

@MainActor
private final class LaserPointerController: ObservableObject {
    @Published private(set) var isPointing = false
    private let motionManager = CMMotionManager()
    private var origin: (yaw: Double, pitch: Double)?

    var isAvailable: Bool {
        motionManager.isDeviceMotionAvailable
    }

    func startPointing(queue: RemoteInputQueue) {
        guard motionManager.isDeviceMotionAvailable, !isPointing else { return }
        origin = nil
        isPointing = true
        motionManager.deviceMotionUpdateInterval = 1.0 / 30.0
        motionManager.startDeviceMotionUpdates(to: .main) { [weak self] motion, _ in
            guard let motion else { return }
            let yaw = motion.attitude.yaw
            let pitch = motion.attitude.pitch
            Task { @MainActor in
                self?.handleSample(yaw: yaw, pitch: pitch, queue: queue)
            }
        }
    }

    private func handleSample(yaw: Double, pitch: Double, queue: RemoteInputQueue) {
        guard isPointing else { return }
        let start: (yaw: Double, pitch: Double)
        if let existing = origin {
            start = existing
        } else {
            start = (yaw: yaw, pitch: pitch)
            origin = start
        }
        let spot = RemoteInputMath.presenterSpot(
            yaw0: start.yaw,
            pitch0: start.pitch,
            yaw: yaw,
            pitch: pitch
        )
        queue.point(.spot(PresenterSpot(x: spot.x, y: spot.y)))
    }

    func stopPointing(queue: RemoteInputQueue) {
        guard isPointing else { return }
        isPointing = false
        origin = nil
        motionManager.stopDeviceMotionUpdates()
        queue.point(.hidden)
    }
}

// MARK: - Multi-Touch UIKit Trackpad Surface

private struct MultiTouchPadSurface: UIViewRepresentable {
    var sensitivity: Double
    let queue: RemoteInputQueue
    let onTouchPoint: @MainActor (CGPoint?) -> Void

    func makeUIView(context: Context) -> TouchpadUIView {
        let view = TouchpadUIView(queue: queue, onTouchPoint: onTouchPoint)
        view.sensitivity = sensitivity
        return view
    }

    func updateUIView(_ uiView: TouchpadUIView, context: Context) {
        uiView.sensitivity = sensitivity
    }
}

@MainActor
private final class TouchpadUIView: UIView, UIGestureRecognizerDelegate {
    private static let scrollNotchPoints: Double = 24.0

    var sensitivity: Double = MobileStore.defaultPointerSpeed
    private let queue: RemoteInputQueue
    private let onTouchPoint: @MainActor (CGPoint?) -> Void

    private var lastPanTimestamp: TimeInterval = 0
    private var lastDragLocation: CGPoint = .zero

    init(queue: RemoteInputQueue, onTouchPoint: @escaping @MainActor (CGPoint?) -> Void) {
        self.queue = queue
        self.onTouchPoint = onTouchPoint
        super.init(frame: .zero)
        backgroundColor = .clear
        isMultipleTouchEnabled = true
        setupRecognizers()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    private func setupRecognizers() {
        let oneFingerPan = UIPanGestureRecognizer(target: self, action: #selector(handleSinglePan(_:)))
        oneFingerPan.minimumNumberOfTouches = 1
        oneFingerPan.maximumNumberOfTouches = 1
        oneFingerPan.delegate = self
        addGestureRecognizer(oneFingerPan)

        let twoFingerPan = UIPanGestureRecognizer(target: self, action: #selector(handleTwoFingerScroll(_:)))
        twoFingerPan.minimumNumberOfTouches = 2
        twoFingerPan.maximumNumberOfTouches = 2
        twoFingerPan.delegate = self
        addGestureRecognizer(twoFingerPan)

        let singleTap = UITapGestureRecognizer(target: self, action: #selector(handleSingleTap(_:)))
        singleTap.numberOfTouchesRequired = 1
        singleTap.numberOfTapsRequired = 1
        addGestureRecognizer(singleTap)

        let twoFingerTap = UITapGestureRecognizer(target: self, action: #selector(handleTwoFingerTap(_:)))
        twoFingerTap.numberOfTouchesRequired = 2
        twoFingerTap.numberOfTapsRequired = 1
        addGestureRecognizer(twoFingerTap)

        let holdDrag = UILongPressGestureRecognizer(target: self, action: #selector(handleHoldDrag(_:)))
        holdDrag.minimumPressDuration = 0.42
        holdDrag.numberOfTouchesRequired = 1
        addGestureRecognizer(holdDrag)
    }

    @objc private func handleSinglePan(_ gesture: UIPanGestureRecognizer) {
        let now = ProcessInfo.processInfo.systemUptime
        switch gesture.state {
        case .began:
            lastPanTimestamp = now
            gesture.setTranslation(.zero, in: self)
            onTouchPoint(gesture.location(in: self))

        case .changed:
            let delta = gesture.translation(in: self)
            gesture.setTranslation(.zero, in: self)
            let dtMs = max(1.0, (now - lastPanTimestamp) * 1000.0)
            lastPanTimestamp = now
            let distance = hypot(Double(delta.x), Double(delta.y))
            let gain = RemoteInputMath.pointerGain(speedPointsPerMs: distance / dtMs, sensitivity: sensitivity)
            queue.move(dx: Double(delta.x) * gain, dy: Double(delta.y) * gain)
            onTouchPoint(gesture.location(in: self))

        case .ended, .cancelled, .failed:
            onTouchPoint(nil)

        default:
            break
        }
    }

    @objc private func handleTwoFingerScroll(_ gesture: UIPanGestureRecognizer) {
        switch gesture.state {
        case .began:
            gesture.setTranslation(.zero, in: self)
            onTouchPoint(nil)

        case .changed:
            let delta = gesture.translation(in: self)
            gesture.setTranslation(.zero, in: self)
            // Natural scrolling matching docs/features.md §9: scrollY > 0 turns wheel up.
            let sx = -Double(delta.x) / Self.scrollNotchPoints
            let sy = Double(delta.y) / Self.scrollNotchPoints
            queue.scroll(scrollX: sx, scrollY: sy)

        default:
            break
        }
    }

    @objc private func handleSingleTap(_ gesture: UITapGestureRecognizer) {
        if gesture.state == .ended {
            queue.add(.inputClick(InputClickPayload(button: InputClickPayload.buttonLeft)))
        }
    }

    @objc private func handleTwoFingerTap(_ gesture: UITapGestureRecognizer) {
        if gesture.state == .ended {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            queue.add(.inputClick(InputClickPayload(button: InputClickPayload.buttonRight)))
        }
    }

    @objc private func handleHoldDrag(_ gesture: UILongPressGestureRecognizer) {
        let loc = gesture.location(in: self)
        switch gesture.state {
        case .began:
            lastDragLocation = loc
            onTouchPoint(loc)
            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
            queue.add(.inputClick(InputClickPayload(button: InputClickPayload.buttonLeft, down: true)))

        case .changed:
            let dx = Double(loc.x - lastDragLocation.x) * sensitivity
            let dy = Double(loc.y - lastDragLocation.y) * sensitivity
            lastDragLocation = loc
            onTouchPoint(loc)
            queue.move(dx: dx, dy: dy)

        case .ended, .cancelled, .failed:
            onTouchPoint(nil)
            queue.add(.inputClick(InputClickPayload(button: InputClickPayload.buttonLeft, up: true)))

        default:
            break
        }
    }
}
#endif
