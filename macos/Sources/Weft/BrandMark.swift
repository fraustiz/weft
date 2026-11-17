// The Weft mark, drawn from brand/final/weft-mark.svg: three rounded
// pixels crossed by a needle, which leaves through the missing fourth.

import SwiftUI

struct BrandMark: View {
    /// Sunset orange, the brand's one colour (brand/README.md).
    static let orange = Color(red: 0xF2 / 255, green: 0x6A / 255, blue: 0x21 / 255)

    var color: Color = BrandMark.orange

    var body: some View {
        Canvas { context, size in
            let scale = min(size.width, size.height) / 100
            let place = CGAffineTransform(scaleX: scale, y: scale)
            let needle = Self.needle.applying(place)
            context.drawLayer { layer in
                for (x, y) in [(20.0, 20.0), (20.0, 52.0), (52.0, 52.0)] {
                    let pixel = Path(
                        roundedRect: CGRect(x: x, y: y, width: 28, height: 28),
                        cornerRadius: 7
                    )
                    layer.fill(pixel.applying(place), with: .color(color))
                }
                // The gap the needle cuts through the pixels.
                layer.blendMode = .destinationOut
                layer.stroke(
                    needle,
                    with: .color(.black),
                    style: StrokeStyle(lineWidth: 5 * scale, lineJoin: .round)
                )
                layer.fill(needle, with: .color(.black))
            }
            context.fill(needle, with: .color(color), style: FillStyle(eoFill: true))
        }
        .aspectRatio(1, contentMode: .fit)
        .accessibilityHidden(true)
    }

    /// The needle and its eye, in the 100 × 100 space of the SVG.
    static let needle: Path = {
        var path = Path()
        path.move(to: .zero)
        path.addCurve(
            to: CGPoint(x: 86, y: -3.2),
            control1: CGPoint(x: 24.64, y: -2.40),
            control2: CGPoint(x: 61.60, y: -3.2)
        )
        path.addArc(
            center: CGPoint(x: 86, y: 0),
            radius: 3.2,
            startAngle: .degrees(-90),
            endAngle: .degrees(90),
            clockwise: false
        )
        path.addCurve(
            to: .zero,
            control1: CGPoint(x: 61.60, y: 3.2),
            control2: CGPoint(x: 24.64, y: 2.40)
        )
        path.closeSubpath()
        path.addRoundedRect(
            in: CGRect(x: 77, y: -0.8, width: 6, height: 1.6),
            cornerSize: CGSize(width: 0.8, height: 0.8)
        )
        let transform = CGAffineTransform(translationX: 14, y: 88)
            .rotated(by: -49.5 * .pi / 180)
        return path.applying(transform)
    }()
}
