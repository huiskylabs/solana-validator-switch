// Test setup file
// This file is run before each test file

// Mock external dependencies if needed
jest.mock("chalk", () => ({
  green: jest.fn((text) => text),
  yellow: jest.fn((text) => text),
  red: jest.fn((text) => text),
  blue: jest.fn((text) => text),
  bold: {
    cyan: jest.fn((text) => text),
  },
}));

// Set test environment
process.env.NODE_ENV = "test";
process.env.SVS_LOG_LEVEL = "error"; // Suppress logs during tests
