import AppKit
import Vision

struct Line: Encodable {
    let text: String
    let x, y, w, h: CGFloat
}

let url = URL(fileURLWithPath: CommandLine.arguments[1])
guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
      let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
    fatalError("Cannot read input image")
}
let request = VNRecognizeTextRequest()
request.recognitionLevel = .accurate
request.recognitionLanguages = ["en-US"]
try VNImageRequestHandler(cgImage: image).perform([request])
let lines = (request.results ?? []).compactMap { item -> Line? in
    guard let value = item.topCandidates(1).first else { return nil }
    let box = item.boundingBox
    return Line(text: value.string, x: box.minX * CGFloat(image.width),
                y: (1 - box.maxY) * CGFloat(image.height),
                w: box.width * CGFloat(image.width), h: box.height * CGFloat(image.height))
}
FileHandle.standardOutput.write(try JSONEncoder().encode(lines))
