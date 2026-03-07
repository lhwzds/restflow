import AppKit

enum StatusBarAppearance {
    static let iconSize = NSSize(width: 20, height: 20)
    static let monochromeColor = NSColor.black
    static let nodeRadiusScale: CGFloat = 0.1404444444
    static let leftNodeXScale: CGFloat = -0.0888888889
    static let leftNodeYScale: CGFloat = 0.4222222222
    static let rightNodeXScale: CGFloat = 0.3851111111
    static let rightNodeYScale: CGFloat = 0.7
    static let connectorCenterXScale: CGFloat = 0.1826391833
    static let connectorCenterYScale: CGFloat = 0.5739556309
    static let connectorLengthScale: CGFloat = 0.3112147932
    static let connectorThicknessScale: CGFloat = 0.0915366913
    static let connectorAngleDegrees: CGFloat = 30.3714127954

    static func loadStatusBarImage() -> NSImage? {
        let image = NSImage(size: iconSize, flipped: false) { rect in
            let canvas = rect.insetBy(dx: 0.8, dy: 0.8)
            let filledCircle = NSBezierPath(ovalIn: canvas)
            monochromeColor.setFill()
            filledCircle.fill()

            let radius = min(canvas.width, canvas.height) * 0.5
            let center = NSPoint(x: canvas.midX, y: canvas.midY)
            let nodeRadius = radius * nodeRadiusScale
            let leftCenter = NSPoint(
                x: center.x + radius * leftNodeXScale,
                y: center.y + radius * leftNodeYScale
            )
            let rightCenter = NSPoint(
                x: center.x + radius * rightNodeXScale,
                y: center.y + radius * rightNodeYScale
            )
            let connectorCenter = CGPoint(
                x: center.x + radius * connectorCenterXScale,
                y: center.y + radius * connectorCenterYScale
            )
            let connectorLength = radius * connectorLengthScale
            let connectorThickness = radius * connectorThicknessScale

            guard let context = NSGraphicsContext.current?.cgContext else {
                return true
            }

            context.saveGState()
            context.setBlendMode(.clear)
            context.setStrokeColor(NSColor.clear.cgColor)
            context.setFillColor(NSColor.clear.cgColor)

            let connectorRect = CGRect(
                x: connectorCenter.x - connectorLength * 0.5,
                y: connectorCenter.y - connectorThickness * 0.5,
                width: connectorLength,
                height: connectorThickness
            )
            let connectorPath = CGPath(
                roundedRect: connectorRect,
                cornerWidth: connectorThickness * 0.5,
                cornerHeight: connectorThickness * 0.5,
                transform: nil
            )
            var transform = CGAffineTransform.identity
            transform = transform.translatedBy(x: connectorCenter.x, y: connectorCenter.y)
            transform = transform.rotated(by: connectorAngleDegrees * .pi / 180.0)
            transform = transform.translatedBy(x: -connectorCenter.x, y: -connectorCenter.y)
            if let rotatedConnector = connectorPath.copy(using: &transform) {
                context.addPath(rotatedConnector)
                context.fillPath()
            }

            context.fillEllipse(
                in: CGRect(
                    x: leftCenter.x - nodeRadius,
                    y: leftCenter.y - nodeRadius,
                    width: nodeRadius * 2,
                    height: nodeRadius * 2
                )
            )
            context.fillEllipse(
                in: CGRect(
                    x: rightCenter.x - nodeRadius,
                    y: rightCenter.y - nodeRadius,
                    width: nodeRadius * 2,
                    height: nodeRadius * 2
                )
            )
            context.restoreGState()

            return true
        }

        image.isTemplate = true
        return image
    }

    static func tooltip(for snapshot: UiSnapshot?) -> String {
        guard let snapshot else {
            return "RestFlow Menu Bar"
        }

        let tasks = snapshot.summary.tasks
        return "RestFlow Menu Bar\nStatus: \(snapshot.daemon.status)\nActive: \(tasks.active)\nQueued: \(tasks.queued)\nCompleted today: \(tasks.completedToday)"
    }
}
