pragma circom 2.1.0;

template PrivateTransaction() {
    signal input amount;
    signal input recipient;
    signal input timestamp;
    signal input merkle_root;
    signal input nullifier;
    signal input block_height;
    signal input sender_balance;
    signal input sender_public_key;
    
    signal balance_check;
    balance_check <== sender_balance - amount;
    
    signal amount_check;
    amount_check <== amount - 1;
}

component main = PrivateTransaction();