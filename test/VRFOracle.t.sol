// SPDX-License-Identifier: MIT
pragma solidity ^0.8.27;

import "forge-std/Test.sol";
import "../contracts/oracle.sol";

/**
 * @title VRFOracleTest
 * @notice Foundry tests for VRFOracle contract focusing on event emissions
 */
contract VRFOracleTest is Test {
    VRFOracle public vrfOracle;

    address public oracle = address(0x1234);
    address public user = address(0x5678);
    uint64 public fee = 0.01 ether;

    // Events to test
    event RandomnessRequested(bytes32 indexed requestId, address indexed requester, uint256 paid);

    event RandomnessFulfilled(bytes32 indexed requestId, uint256 randomness);

    function setUp() public {
        vrfOracle = new VRFOracle(oracle, fee);
        vm.deal(user, 1 ether);
    }

    function testRequestRandomnessEmitsEvent() public {
        vm.startPrank(user);

        // Calculate expected requestId
        bytes32 expectedRequestId = keccak256(abi.encodePacked(user, uint256(0)));

        // Expect the event
        vm.expectEmit(true, true, false, true);
        emit RandomnessRequested(expectedRequestId, user, fee);

        // Make the request
        bytes32 requestId = vrfOracle.requestRandomness{value: fee}();

        // Verify the request ID matches
        assertEq(requestId, expectedRequestId);

        vm.stopPrank();
    }

    function testMultipleRequestsEmitCorrectEvents() public {
        vm.startPrank(user);

        // First request
        bytes32 expectedRequestId1 = keccak256(abi.encodePacked(user, uint256(0)));
        vm.expectEmit(true, true, false, true);
        emit RandomnessRequested(expectedRequestId1, user, fee);
        vrfOracle.requestRandomness{value: fee}();

        // Second request
        bytes32 expectedRequestId2 = keccak256(abi.encodePacked(user, uint256(1)));
        vm.expectEmit(true, true, false, true);
        emit RandomnessRequested(expectedRequestId2, user, fee);
        vrfOracle.requestRandomness{value: fee}();

        vm.stopPrank();
    }

    function testInsufficientFeeRevert() public {
        vm.startPrank(user);

        vm.expectRevert(VRFOracle.InsufficientFee.selector);
        vrfOracle.requestRandomness{value: fee - 1}();

        vm.stopPrank();
    }

    /**
     * @notice Test RandomnessFulfilled event is emitted correctly
     */
    function testFulfillRandomnessEmitsEvent() public {
        // User makes a request
        vm.prank(user);
        bytes32 requestId = vrfOracle.requestRandomness{value: fee}();

        // Oracle fulfills the request
        vm.startPrank(oracle);

        uint256 randomValue = 12345678;
        bytes memory proof = "";

        // Expect the event
        vm.expectEmit(true, false, false, true);
        emit RandomnessFulfilled(requestId, randomValue);

        // Fulfill the request
        vrfOracle.fulfillRandomness(requestId, randomValue, proof);

        vm.stopPrank();
    }

    /**
     * @notice Test overpayment still emits correct paid amount
     */
    function testOverpaymentEmitsCorrectAmount() public {
        vm.startPrank(user);

        uint256 overpayment = fee * 2;
        bytes32 expectedRequestId = keccak256(abi.encodePacked(user, uint256(0)));

        // Expect event with actual paid amount
        vm.expectEmit(true, true, false, true);
        emit RandomnessRequested(expectedRequestId, user, overpayment);

        vrfOracle.requestRandomness{value: overpayment}();

        vm.stopPrank();
    }

    /**
     * @notice Test that no events are emitted on failed transactions
     */
    function testNoEventOnInsufficientFee() public {
        vm.startPrank(user);

        // Should not emit any events
        vm.expectRevert(VRFOracle.InsufficientFee.selector);
        vrfOracle.requestRandomness{value: fee - 1}();

        vm.stopPrank();
    }

    /**
     * @notice Test that no events are emitted when non-oracle tries to fulfill
     */
    function testNoEventOnUnauthorizedFulfill() public {
        // User makes a request
        vm.prank(user);
        bytes32 requestId = vrfOracle.requestRandomness{value: fee}();

        // Non-oracle tries to fulfill
        vm.prank(user);
        vm.expectRevert(VRFOracle.OnlyOracle.selector);
        vrfOracle.fulfillRandomness(requestId, 12345, "");
    }
}
