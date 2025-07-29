-- Drop the table if it exists
DROP TABLE IF EXISTS zamaoracle_vrf_oracle.pending_requests;

-- Create pending_requests table for durable request queue
CREATE TABLE zamaoracle_vrf_oracle.pending_requests (
    request_id BYTEA PRIMARY KEY,
    contract_address VARCHAR(42) NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    processing_started_at TIMESTAMP WITH TIME ZONE,
    fulfilled_at TIMESTAMP WITH TIME ZONE,
    retry_count INTEGER DEFAULT 0,
    max_retries INTEGER DEFAULT 5,
    last_error TEXT,
    network VARCHAR(50) NOT NULL,
    CONSTRAINT valid_status CHECK (status IN ('pending', 'processing', 'fulfilled', 'failed'))
);

-- Create indexes for efficient queue processing
CREATE INDEX IF NOT EXISTS idx_pending_requests_status ON zamaoracle_vrf_oracle.pending_requests(status);
CREATE INDEX IF NOT EXISTS idx_pending_requests_created_at ON zamaoracle_vrf_oracle.pending_requests(created_at);
CREATE INDEX IF NOT EXISTS idx_pending_requests_processing ON zamaoracle_vrf_oracle.pending_requests(status, created_at)
    WHERE status = 'pending' OR status = 'processing';
CREATE INDEX IF NOT EXISTS idx_pending_requests_processing_timeout ON zamaoracle_vrf_oracle.pending_requests(processing_started_at)
    WHERE status = 'processing' AND processing_started_at IS NOT NULL;

-- Create index for efficient latency queries
CREATE INDEX IF NOT EXISTS idx_pending_requests_fulfilled_at
ON zamaoracle_vrf_oracle.pending_requests(fulfilled_at)
WHERE status = 'fulfilled';

-- Function to update the updated_at timestamp
CREATE OR REPLACE FUNCTION zamaoracle_vrf_oracle.update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Trigger to automatically update updated_at
DROP TRIGGER IF EXISTS update_pending_requests_updated_at ON zamaoracle_vrf_oracle.pending_requests;
CREATE TRIGGER update_pending_requests_updated_at
    BEFORE UPDATE ON zamaoracle_vrf_oracle.pending_requests
    FOR EACH ROW
    EXECUTE FUNCTION zamaoracle_vrf_oracle.update_updated_at_column();
