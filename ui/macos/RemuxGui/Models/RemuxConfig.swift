import Foundation

struct RemuxConfig: Codable {
    static let defaultOutputFolder = "SRC-FOLDER"

    var withAudio: Bool = true
    var withVideo: Bool = true
    var forceRate: UInt32 = 0
    var fastStart: Bool = false
    var outputFolder: String = Self.defaultOutputFolder
    var mp4: Bool = true
    var videoTrack: UInt16 = 0
    var baseName: String?

    enum CodingKeys: String, CodingKey {
        case withAudio = "with_audio"
        case withVideo = "with_video"
        case forceRate = "force_rate"
        case fastStart = "fast_start"
        case outputFolder = "output_folder"
        case mp4
        case videoTrack = "video_track"
        case baseName = "base_name"
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(withAudio, forKey: .withAudio)
        try container.encode(withVideo, forKey: .withVideo)
        try container.encode(forceRate, forKey: .forceRate)
        try container.encode(fastStart, forKey: .fastStart)
        try container.encode(outputFolder, forKey: .outputFolder)
        try container.encode(mp4, forKey: .mp4)
        try container.encode(videoTrack, forKey: .videoTrack)
        try container.encodeIfPresent(baseName, forKey: .baseName)
    }
}
