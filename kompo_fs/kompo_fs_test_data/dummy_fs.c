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
