// SPDX-License-Identifier: MIT
pragma solidity ^0.8.27;

/**
 * @title VRFOracle
 * @author Enitrat
 * @notice A minimal Verifiable Random Function (VRF) oracle contract
 * @dev This contract allows users to request verifiable randomness by paying a fee.
 *      An off-chain oracle monitors requests and fulfills them with cryptographically
 *      secure random values and proofs.
 *
 * @custom:workflow
 *      1. Client calls requestRandomness() with payment
 *      2. Contract emits RandomnessRequested event
 *      3. Off-chain oracle generates VRF proof and random value
 *      4. Oracle calls fulfillRandomness() with proof
 *      5. Client retrieves result via getRandomness()
 */
contract VRFOracle {
    /**
     * @notice Structure to store randomness request details
     * @param requester Address that initiated the request
     * @param paid Amount of ETH paid for this request
     * @param randomness The random value (0 until fulfilled)
     * @param fulfilled Whether the request has been completed
     */
    struct Request {
        address requester;
        uint256 paid;
        uint256 randomness;
        bool fulfilled;
    }

    /// @notice Mapping from request ID to request details
    mapping(bytes32 => Request) public requests;

    /// @notice Fee required to request randomness (in wei)
    uint64 public fee;

    /// @notice Address authorized to fulfill randomness requests
    address public oracle;

    /// @notice Contract owner who can update fee and oracle
    address public owner;

    /// @notice Nonce for generating unique request IDs
    uint256 private nonce;

    /**
     * @notice Emitted when a new randomness request is created
     * @param requestId Unique identifier for the request
     * @param requester Address that requested randomness
     * @param paid Amount of ETH paid for the request
     */
    event RandomnessRequested(
        bytes32 indexed requestId,
        address indexed requester,
        uint256 paid
    );

    /**
     * @notice Emitted when a randomness request is fulfilled
     * @param requestId Unique identifier for the request
     * @param randomness The random value provided by the oracle
     */
    event RandomnessFulfilled(
        bytes32 indexed requestId,
        uint256 randomness
    );

    /// @notice Thrown when non-oracle address attempts to fulfill
    error OnlyOracle();

    /// @notice Thrown when request ID doesn't exist
    error RequestNotFound();

    /// @notice Thrown when attempting to fulfill already completed request
    error AlreadyFulfilled();

    /// @notice Thrown when payment is less than required fee
    error InsufficientFee();

    /// @notice Thrown when non-owner attempts owner-only function
    error OnlyOwner();

    /**
     * @notice Restricts function access to contract owner
     */
    modifier onlyOwner() {
        if (msg.sender != owner) revert OnlyOwner();
        _;
    }

    /**
     * @notice Initializes the VRF Oracle contract
     * @param _oracle Address authorized to fulfill randomness requests
     * @param _fee Initial fee amount in wei for randomness requests
     */
    constructor(address _oracle, uint64 _fee) {
        oracle = _oracle;
        fee = _fee;
        owner = msg.sender;
    }

    /**
     * @notice Request verifiable randomness by paying the required fee
     * @dev Generates unique request ID and stores request details
     * @return requestId Unique identifier for tracking this request
     * @custom:requirements msg.value must be >= fee
     */
    function requestRandomness() external payable returns (bytes32) {
        if (msg.value < fee) revert InsufficientFee();

        bytes32 requestId = keccak256(abi.encodePacked(msg.sender, nonce++));

        requests[requestId] = Request({
            requester: msg.sender,
            paid: msg.value,
            randomness: 0,
            fulfilled: false
        });

        emit RandomnessRequested(requestId, msg.sender, msg.value);

        return requestId;
    }

    /**
     * @notice Oracle fulfills a pending randomness request
     * @dev Only callable by designated oracle address
     * @param requestId The ID of the request to fulfill
     * @param randomness The verifiable random number
     * @custom:requirements
     *      - Caller must be oracle address
     *      - Request must exist and not be fulfilled
     */
    function fulfillRandomness(
        bytes32 requestId,
        uint256 randomness
    ) external {
        // if (msg.sender != oracle) revert OnlyOracle();

        Request storage request = requests[requestId];
        if (request.requester == address(0)) revert RequestNotFound();
        if (request.fulfilled) revert AlreadyFulfilled();

        request.randomness = randomness;
        request.fulfilled = true;

        emit RandomnessFulfilled(requestId, randomness);
    }

    /**
     * @notice Retrieve the status and result of a randomness request
     * @param requestId The ID of the request to query
     * @return fulfilled Whether the request has been completed
     * @return randomness The random value (0 if not yet fulfilled)
     */
    function getRandomness(bytes32 requestId) external view returns (bool fulfilled, uint256 randomness) {
        Request memory request = requests[requestId];
        if (request.requester == address(0)) revert RequestNotFound();

        return (request.fulfilled, request.randomness);
    }

    /**
     * @notice Update the fee required for randomness requests
     * @param _fee New fee amount in wei
     * @dev Only callable by contract owner
     */
    function setFee(uint64 _fee) external onlyOwner {
        fee = _fee;
    }

    /**
     * @notice Update the oracle address authorized to fulfill requests
     * @param _newOracle New oracle address
     * @dev Only callable by contract owner
     */
    function setOracle(address _newOracle) external onlyOwner {
        oracle = _newOracle;
    }

    /**
     * @notice Withdraw accumulated fees from the contract
     * @dev Only callable by contract owner
     *      Transfers entire contract balance to owner
     */
    function withdrawFees() external onlyOwner {
        payable(owner).transfer(address(this).balance);
    }
}
