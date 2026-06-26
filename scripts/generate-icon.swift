import AppKit

let root = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
let icons = root.appendingPathComponent("src-tauri/icons")
try FileManager.default.createDirectory(at: icons, withIntermediateDirectories: true)

let size = NSSize(width: 1024, height: 1024)
let image = NSImage(size: size)

image.lockFocus()

let rect = NSRect(origin: .zero, size: size)
let background = NSBezierPath(roundedRect: rect.insetBy(dx: 64, dy: 64), xRadius: 220, yRadius: 220)

NSColor(calibratedRed: 0.04, green: 0.08, blue: 0.15, alpha: 1).setFill()
background.fill()

let gradient = NSGradient(colors: [
    NSColor(calibratedRed: 0.41, green: 0.9, blue: 0.75, alpha: 1),
    NSColor(calibratedRed: 0.57, green: 0.69, blue: 1, alpha: 1)
])!
gradient.draw(in: background, angle: 135)

NSColor(calibratedWhite: 1, alpha: 0.24).setStroke()
background.lineWidth = 18
background.stroke()

let panel = NSBezierPath(roundedRect: NSRect(x: 230, y: 260, width: 564, height: 504), xRadius: 72, yRadius: 72)
NSColor(calibratedRed: 0.05, green: 0.09, blue: 0.17, alpha: 0.9).setFill()
panel.fill()

let lineColor = NSColor(calibratedRed: 0.85, green: 0.93, blue: 1, alpha: 0.95)
lineColor.setStroke()

for y in [640, 512, 384] {
    let path = NSBezierPath()
    path.move(to: NSPoint(x: 330, y: y))
    path.line(to: NSPoint(x: 690, y: y))
    path.lineWidth = 42
    path.lineCapStyle = .round
    path.stroke()
}

for point in [NSPoint(x: 330, y: 640), NSPoint(x: 530, y: 512), NSPoint(x: 690, y: 384)] {
    let dot = NSBezierPath(ovalIn: NSRect(x: point.x - 46, y: point.y - 46, width: 92, height: 92))
    NSColor(calibratedRed: 0.41, green: 0.9, blue: 0.75, alpha: 1).setFill()
    dot.fill()
}

let bolt = NSBezierPath()
bolt.move(to: NSPoint(x: 580, y: 746))
bolt.line(to: NSPoint(x: 460, y: 522))
bolt.line(to: NSPoint(x: 568, y: 522))
bolt.line(to: NSPoint(x: 486, y: 290))
bolt.line(to: NSPoint(x: 676, y: 574))
bolt.line(to: NSPoint(x: 556, y: 574))
bolt.close()
NSColor(calibratedRed: 1, green: 0.88, blue: 0.32, alpha: 1).setFill()
bolt.fill()

image.unlockFocus()

guard let tiff = image.tiffRepresentation,
      let bitmap = NSBitmapImageRep(data: tiff),
      let png = bitmap.representation(using: .png, properties: [:]) else {
    fatalError("Failed to render icon")
}

try png.write(to: icons.appendingPathComponent("icon.png"))
