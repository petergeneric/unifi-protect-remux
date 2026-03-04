import Foundation

enum ProgressEventType: String, Decodable {
    case log
    case fileStarted = "file_started"
    case partitionsFound = "partitions_found"
    case partitionStarted = "partition_started"
    case outputGenerated = "output_generated"
    case partitionError = "partition_error"
    case fileCompleted = "file_completed"
}

struct ProgressEvent: Decodable {
    let type: ProgressEventType
    let level: String?
    let message: String?
    let path: String?
    let count: Int?
    let index: Int?
    let total: Int?
    let error: String?
    let outputs: [String]?
    let errors: [String]?
}
