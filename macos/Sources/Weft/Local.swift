// A view's own value, kept across updates as @State keeps it.
//
// With the Command Line Tools only, the macOS 27 SDK resolves `@State` to
// a macro whose plugin (SwiftUIMacros) ships with Xcode alone, so this app
// does not use it: `@StateObject private var x = Local(…)`, then `x.value`
// and `$x.value`.

import SwiftUI

@MainActor
final class Local<Value>: ObservableObject {
    @Published var value: Value

    init(_ value: Value) {
        self.value = value
    }
}
