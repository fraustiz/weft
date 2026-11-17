// This Mac's output volume, through Core Audio: the default output
// device's main volume and mute, read, watched and set
// (docs/features.md section 6, `volume` and `volume.set`).

import AudioToolbox
import CoreAudio
import Foundation

@MainActor
final class SystemVolume {
    /// Called on the main thread with the level (0 to 100) and the mute,
    /// whenever either changes and once at start.
    var onChange: ((UInt8, Bool) -> Void)?

    private var device = AudioObjectID(kAudioObjectUnknown)
    private var last: (UInt8, Bool)?
    private var deviceListener: AudioObjectPropertyListenerBlock?
    private var valueListener: AudioObjectPropertyListenerBlock?

    private static var defaultOutput = AudioObjectPropertyAddress(
        mSelector: kAudioHardwarePropertyDefaultOutputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain
    )
    private static var volume = AudioObjectPropertyAddress(
        mSelector: kAudioHardwareServiceDeviceProperty_VirtualMainVolume,
        mScope: kAudioDevicePropertyScopeOutput,
        mElement: kAudioObjectPropertyElementMain
    )
    private static var mute = AudioObjectPropertyAddress(
        mSelector: kAudioDevicePropertyMute,
        mScope: kAudioDevicePropertyScopeOutput,
        mElement: kAudioObjectPropertyElementMain
    )

    func start() {
        let listener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            MainActor.assumeIsolated { self?.followDefaultDevice() }
        }
        deviceListener = listener
        AudioObjectAddPropertyListenerBlock(
            AudioObjectID(kAudioObjectSystemObject), &Self.defaultOutput, .main, listener
        )
        followDefaultDevice()
    }

    /// The current level and mute, if the device has them.
    func read() -> (level: UInt8, muted: Bool)? {
        guard device != kAudioObjectUnknown else { return nil }
        var scalar = Float32(0)
        var size = UInt32(MemoryLayout<Float32>.size)
        guard AudioObjectHasProperty(device, &Self.volume),
              AudioObjectGetPropertyData(device, &Self.volume, 0, nil, &size, &scalar) == noErr else {
            return nil
        }
        var muted = UInt32(0)
        size = UInt32(MemoryLayout<UInt32>.size)
        if !AudioObjectHasProperty(device, &Self.mute)
            || AudioObjectGetPropertyData(device, &Self.mute, 0, nil, &size, &muted) != noErr {
            muted = 0
        }
        let level = UInt8((min(max(scalar, 0), 1) * 100).rounded())
        return (level, muted != 0)
    }

    /// Sets the level (0 to 100) and the mute; `nil` leaves it.
    func set(level: UInt8?, muted: Bool?) {
        guard device != kAudioObjectUnknown else { return }
        if let level {
            var scalar = Float32(min(level, 100)) / 100
            var settable = DarwinBoolean(false)
            if AudioObjectIsPropertySettable(device, &Self.volume, &settable) == noErr, settable.boolValue {
                AudioObjectSetPropertyData(device, &Self.volume, 0, nil, UInt32(MemoryLayout<Float32>.size), &scalar)
            }
        }
        if let muted {
            var value = UInt32(muted ? 1 : 0)
            var settable = DarwinBoolean(false)
            if AudioObjectIsPropertySettable(device, &Self.mute, &settable) == noErr, settable.boolValue {
                AudioObjectSetPropertyData(device, &Self.mute, 0, nil, UInt32(MemoryLayout<UInt32>.size), &value)
            }
        }
    }

    /// Moves the watch to the current default output device.
    private func followDefaultDevice() {
        if device != kAudioObjectUnknown, let valueListener {
            AudioObjectRemovePropertyListenerBlock(device, &Self.volume, .main, valueListener)
            AudioObjectRemovePropertyListenerBlock(device, &Self.mute, .main, valueListener)
        }
        var id = AudioObjectID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        let status = AudioObjectGetPropertyData(
            AudioObjectID(kAudioObjectSystemObject), &Self.defaultOutput, 0, nil, &size, &id
        )
        device = status == noErr ? id : AudioObjectID(kAudioObjectUnknown)
        guard device != kAudioObjectUnknown else { return }
        let listener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            MainActor.assumeIsolated { self?.report() }
        }
        valueListener = listener
        AudioObjectAddPropertyListenerBlock(device, &Self.volume, .main, listener)
        AudioObjectAddPropertyListenerBlock(device, &Self.mute, .main, listener)
        report()
    }

    private func report() {
        guard let now = read() else { return }
        if let last, last == (now.level, now.muted) { return }
        last = (now.level, now.muted)
        onChange?(now.level, now.muted)
    }
}
