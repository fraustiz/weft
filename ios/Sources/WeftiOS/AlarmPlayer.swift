// Synthesized 1-second two-tone WAV alarm played via AVAudioPlayer under the
// `.playback` audio session category so a peer's `ring` plays audibly even
// when the iPhone Ring/Silent switch is silent (docs/features.md §3).

#if canImport(UIKit)
import AVFoundation
import Foundation
import WeftMobileKit

@MainActor
final class AlarmPlayer {
    private var player: AVAudioPlayer?
    private var autoStopTask: Task<Void, Never>?
    var onAutoStop: (() -> Void)?

    var isPlaying: Bool {
        player?.isPlaying == true
    }

    func start() {
        autoStopTask?.cancel()
        if player == nil {
            let session = AVAudioSession.sharedInstance()
            try? session.setCategory(.playback, mode: .default, options: [.duckOthers])
            try? session.setActive(true)
            if let created = try? AVAudioPlayer(data: Self.alarmWAV) {
                created.numberOfLoops = -1
                created.volume = 1.0
                created.prepareToPlay()
                created.play()
                player = created
            }
        }
        // Automatically stop after 60 seconds per docs/features.md section 3.
        autoStopTask = Task { [weak self] in
            try? await Task.sleep(nanoseconds: 60_000_000_000)
            guard !Task.isCancelled, let self else { return }
            self.stop()
            self.onAutoStop?()
        }
    }

    func stop() {
        autoStopTask?.cancel()
        autoStopTask = nil
        if let player {
            player.stop()
            self.player = nil
            try? AVAudioSession.sharedInstance().setActive(false, options: [.notifyOthersOnDeactivation])
        }
    }

    /// 1-second 22,050 Hz 16-bit mono PCM WAV: two short fifth-apart tones
    /// (880 Hz and 1320 Hz, 150 ms each) followed by silence, matching macOS
    /// and Windows Weft clients.
    static let alarmWAV: Data = {
        let rate = 22_050.0
        func tone(_ frequency: Double, _ seconds: Double) -> [Int16] {
            let count = Int(rate * seconds)
            return (0..<count).map { n in
                let phase = (Double(n) * frequency / rate).truncatingRemainder(dividingBy: 1.0)
                let wave = min(max(sin(phase * 2.0 * .pi) * 3.0, -1.0), 1.0)
                let edge = min(Double(min(n, count - n)) / (rate * 0.005), 1.0)
                return Int16(wave * edge * 0.9 * Double(Int16.max))
            }
        }
        func silence(_ seconds: Double) -> [Int16] {
            Array(repeating: 0, count: Int(rate * seconds))
        }

        let samples = tone(880.0, 0.15) + silence(0.05) + tone(1320.0, 0.15) + silence(0.65)
        var data = Data(capacity: 44 + samples.count * 2)
        func appendLE<T: FixedWidthInteger>(_ value: T) {
            withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) }
        }
        let pcmBytes = UInt32(samples.count * 2)
        data.append(contentsOf: Array("RIFF".utf8))
        appendLE(UInt32(36) + pcmBytes)
        data.append(contentsOf: Array("WAVE".utf8))
        data.append(contentsOf: Array("fmt ".utf8))
        appendLE(UInt32(16))
        appendLE(UInt16(1)) // PCM
        appendLE(UInt16(1)) // Mono
        appendLE(UInt32(rate))
        appendLE(UInt32(rate) * 2)
        appendLE(UInt16(2))
        appendLE(UInt16(16))
        data.append(contentsOf: Array("data".utf8))
        appendLE(pcmBytes)
        for sample in samples {
            appendLE(sample)
        }
        return data
    }()
}
#endif
