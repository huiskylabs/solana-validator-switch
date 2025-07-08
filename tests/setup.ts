// Test setup file
// This file is run before each test file

// Mock external dependencies if needed
jest.mock("chalk", () => ({
  __esModule: true,
  default: {
    green: jest.fn((text) => text),
    yellow: jest.fn((text) => text),
    red: jest.fn((text) => text),
    blue: jest.fn((text) => text),
    cyan: jest.fn((text) => text),
    gray: jest.fn((text) => text),
    white: jest.fn((text) => text),
    bold: jest.fn((text) => text),
  },
  green: jest.fn((text) => text),
  yellow: jest.fn((text) => text),
  red: jest.fn((text) => text),
  blue: jest.fn((text) => text),
  cyan: jest.fn((text) => text),
  gray: jest.fn((text) => text),
  white: jest.fn((text) => text),
  bold: jest.fn((text) => text),
}));

// Mock ora (spinner library)
jest.mock("ora", () => ({
  __esModule: true,
  default: jest.fn(() => ({
    start: jest.fn().mockReturnThis(),
    succeed: jest.fn().mockReturnThis(),
    fail: jest.fn().mockReturnThis(),
    stop: jest.fn().mockReturnThis(),
    text: "",
  })),
}));

// Mock inquirer
jest.mock("inquirer", () => ({
  __esModule: true,
  default: {
    prompt: jest.fn(),
  },
}));

// Set test environment
process.env.NODE_ENV = "test";
process.env.SVS_LOG_LEVEL = "error"; // Suppress logs during tests

// Global test utilities
global.setTimeout = setTimeout;
