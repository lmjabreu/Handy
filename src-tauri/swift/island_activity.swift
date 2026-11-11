import ActivityKit
import AppKit
import Foundation

// Drives Handy's Live Activity in the Dynamic Island (menu bar on Macs
// without one). Rust calls these from the main thread; each takes the
// content state as JSON matching HandyIslandAttributes.ContentState.

@available(macOS 27.0, *)
private enum Island {
    static var activity: Activity<HandyIslandAttributes>?

    static func decode(_ json: UnsafePointer<CChar>) -> HandyIslandAttributes.ContentState? {
        let data = Data(String(cString: json).utf8)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .secondsSince1970
        return try? decoder.decode(HandyIslandAttributes.ContentState.self, from: data)
    }
}

/// 1 when this Mac has a camera housing at the top of its built-in display
/// and Live Activities are enabled; Handy then uses the island.
@_cdecl("handy_island_supported")
public func handy_island_supported() -> Int32 {
    guard #available(macOS 27.0, *) else { return 0 }
    let enabled = ActivityAuthorizationInfo().areActivitiesEnabled
    let hasHousing = NSScreen.screens.contains { $0.auxiliaryTopLeftArea != nil }
    NSLog("HandyIsland supported: activitiesEnabled=%d housing=%d", enabled ? 1 : 0, hasHousing ? 1 : 0)
    return enabled && hasHousing ? 1 : 0
}

/// Starts the activity, or updates it if one is running. Returns 1 on
/// success. Starting needs a Handy window on screen (the overlay panel), or
/// ActivityKit refuses with `visibility`; the error goes to `error_out`.
@_cdecl("handy_island_start")
public func handy_island_start(
    _ json: UnsafePointer<CChar>, _ errorOut: UnsafeMutablePointer<CChar>, _ errorLen: Int32
) -> Int32 {
    guard #available(macOS 27.0, *) else { return 0 }
    guard let state = Island.decode(json) else { return 0 }
    if let activity = Island.activity, activity.activityState == .active {
        Task { await activity.update(.init(state: state, staleDate: nil)) }
        return 1
    }
    do {
        let activity = try Activity.request(
            attributes: HandyIslandAttributes(), content: .init(state: state, staleDate: nil))
        Island.activity = activity
        NSLog("HandyIsland requested id=%@ state=%@", activity.id, "\(activity.activityState)")
        Task {
            for await change in activity.activityStateUpdates {
                NSLog("HandyIsland id=%@ state -> %@", activity.id, "\(change)")
            }
        }
        return 1
    } catch {
        let message = "\(type(of: error)): \(error)"
        _ = message.withCString { strncpy(errorOut, $0, Int(errorLen) - 1) }
        return 0
    }
}

@_cdecl("handy_island_update")
public func handy_island_update(_ json: UnsafePointer<CChar>) {
    guard #available(macOS 27.0, *) else { return }
    guard let activity = Island.activity, let state = Island.decode(json) else { return }
    Task { await activity.update(.init(state: state, staleDate: nil)) }
}

@_cdecl("handy_island_end")
public func handy_island_end() {
    guard #available(macOS 27.0, *) else { return }
    guard let activity = Island.activity else { return }
    Island.activity = nil
    NSLog("HandyIsland end id=%@", activity.id)
    Task { await activity.end(nil, dismissalPolicy: .immediate) }
}
