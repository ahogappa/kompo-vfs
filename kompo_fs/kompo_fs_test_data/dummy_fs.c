// Test data for kompo_fs tests
// Paths: "/test/hello.txt" and "/test/world.txt"
const char PATHS[] = "/test/hello.txt\0/test/world.txt";
const int PATHS_SIZE = 32;  // includes both null terminators

// File contents: "Hello, World!" (13 bytes) and "Test Content" (12 bytes)
const char FILES[] = "Hello, World!Test Content";
const int FILES_SIZE = 25;

// Offsets for each file: [0, 13, 25]
const unsigned long long FILES_SIZES[] = {0, 13, 25};

// Working directory
const char WD[] = "/test";

// Compression support symbols (compression disabled for tests)
const int COMPRESSION_ENABLED = 0;
const char COMPRESSED_FILES[] = "";
const int COMPRESSED_FILES_SIZE = 0;
const unsigned long long COMPRESSED_SIZES[] = {0};
char FILES_BUFFER[1] = {0};
const int FILES_BUFFER_SIZE = 0;
const unsigned long long ORIGINAL_SIZES[] = {0};
