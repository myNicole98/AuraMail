package database

import (
	"bytes"
	"encoding/binary"
	"errors"

	bolt "go.etcd.io/bbolt"

	"codeberg.org/emersion/hydroxide/protonmail"
)

var (
	mailboxMetadataBucket = []byte("mailbox_metadata")
	uidValidityKey        = []byte("uid_validity")
	uidNextKey            = []byte("uid_next")
)

func serializeUID(uid uint32) []byte {
	b := make([]byte, 4)
	binary.BigEndian.PutUint32(b, uid)
	return b
}

func unserializeUID(b []byte) uint32 {
	return binary.BigEndian.Uint32(b)
}

func mailboxMetadata(tx *bolt.Tx, labelID string) (*bolt.Bucket, error) {
	root, err := tx.CreateBucketIfNotExists(mailboxMetadataBucket)
	if err != nil {
		return nil, err
	}
	return root.CreateBucketIfNotExists([]byte(labelID))
}

func initialMailboxUIDNext(tx *bolt.Tx, labelID string) uint32 {
	next := uint32(1)
	root := tx.Bucket(mailboxesBucket)
	if root == nil {
		return next
	}
	b := root.Bucket([]byte(labelID))
	if b == nil {
		return next
	}

	if k, _ := b.Cursor().Last(); k != nil {
		next = unserializeUID(k) + 1
	}
	if seq := b.Sequence() + 1; seq > uint64(next) {
		if seq > uint64(^uint32(0)) {
			return ^uint32(0)
		}
		next = uint32(seq)
	}
	return next
}

func ensureMailboxMetadata(tx *bolt.Tx, labelID string) (*bolt.Bucket, error) {
	meta, err := mailboxMetadata(tx, labelID)
	if err != nil {
		return nil, err
	}

	if meta.Get(uidValidityKey) == nil {
		if err := meta.Put(uidValidityKey, serializeUID(1)); err != nil {
			return nil, err
		}
	}
	if meta.Get(uidNextKey) == nil {
		next := initialMailboxUIDNext(tx, labelID)
		if err := meta.Put(uidNextKey, serializeUID(next)); err != nil {
			return nil, err
		}
	}
	return meta, nil
}

func mailboxUIDValidity(tx *bolt.Tx, labelID string) uint32 {
	root := tx.Bucket(mailboxMetadataBucket)
	if root == nil {
		return 1
	}
	meta := root.Bucket([]byte(labelID))
	if meta == nil {
		return 1
	}
	v := meta.Get(uidValidityKey)
	if v == nil {
		return 1
	}
	return unserializeUID(v)
}

func mailboxUIDNext(tx *bolt.Tx, labelID string) uint32 {
	root := tx.Bucket(mailboxMetadataBucket)
	if root != nil {
		if meta := root.Bucket([]byte(labelID)); meta != nil {
			if v := meta.Get(uidNextKey); v != nil {
				return unserializeUID(v)
			}
		}
	}
	return initialMailboxUIDNext(tx, labelID)
}

func mailboxCreateMessage(tx *bolt.Tx, b *bolt.Bucket, labelID, apiID string) (seqNum uint32, err error) {
	want := []byte(apiID)
	c := b.Cursor()
	var n uint32 = 1
	for k, v := c.First(); k != nil; k, v = c.Next() {
		if bytes.Equal(v, want) {
			return n, nil
		}
		n++
	}

	meta, err := ensureMailboxMetadata(tx, labelID)
	if err != nil {
		return 0, err
	}
	uid := unserializeUID(meta.Get(uidNextKey))
	if err := b.Put(serializeUID(uid), want); err != nil {
		return 0, err
	}
	if err := meta.Put(uidNextKey, serializeUID(uid+1)); err != nil {
		return 0, err
	}
	return n, nil
}

func mailboxDeleteMessage(b *bolt.Bucket, apiID string) (seqNum uint32, err error) {
	want := []byte(apiID)
	c := b.Cursor()
	var n uint32 = 1
	for k, v := c.First(); k != nil; k, v = c.Next() {
		if bytes.Equal(v, want) {
			return n, b.Delete(k)
		}
		n++
	}
	return 0, nil
}

type Mailbox struct {
	labelID string
	u       *User
}

func (mbox *Mailbox) bucket(tx *bolt.Tx) (*bolt.Bucket, error) {
	b := tx.Bucket(mailboxesBucket)
	if b == nil {
		return nil, errors.New("cannot find mailboxes bucket")
	}
	b = b.Bucket([]byte(mbox.labelID))
	if b == nil {
		return nil, errors.New("cannot find mailbox bucket")
	}
	return b, nil
}

func (mbox *Mailbox) Sync(messages []*protonmail.Message) error {
	return mbox.u.db.Update(func(tx *bolt.Tx) error {
		b, err := mbox.bucket(tx)
		if err != nil {
			return err
		}

		for _, msg := range messages {
			if _, err := mailboxCreateMessage(tx, b, mbox.labelID, msg.ID); err != nil {
				return err
			}
		}

		return userSync(tx, messages)
	})
}

// Reconcile removes API-ID to UID entries not in the complete remote set.
func (mbox *Mailbox) Reconcile(apiIDs []string) error {
	keep := make(map[string]struct{}, len(apiIDs))
	for _, apiID := range apiIDs {
		keep[apiID] = struct{}{}
	}

	return mbox.u.db.Update(func(tx *bolt.Tx) error {
		b, err := mbox.bucket(tx)
		if err != nil {
			return err
		}

		var stale [][]byte
		c := b.Cursor()
		for k, v := c.First(); k != nil; k, v = c.Next() {
			if _, ok := keep[string(v)]; !ok {
				stale = append(stale, append([]byte(nil), k...))
			}
		}
		for _, k := range stale {
			if err := b.Delete(k); err != nil {
				return err
			}
		}
		return nil
	})
}

func (mbox *Mailbox) UidNext() (uint32, error) {
	var uid uint32
	err := mbox.u.db.View(func(tx *bolt.Tx) error {
		uid = mailboxUIDNext(tx, mbox.labelID)
		return nil
	})
	return uid, err
}

func (mbox *Mailbox) UidValidity() (uint32, error) {
	var valid uint32
	err := mbox.u.db.View(func(tx *bolt.Tx) error {
		valid = mailboxUIDValidity(tx, mbox.labelID)
		return nil
	})
	return valid, err
}

func (mbox *Mailbox) FromUid(uid uint32) (apiID string, err error) {
	err = mbox.u.db.View(func(tx *bolt.Tx) error {
		b, err := mbox.bucket(tx)
		if err != nil {
			return err
		}

		k := serializeUID(uid)
		v := b.Get(k)
		if v == nil {
			return ErrNotFound
		}
		apiID = string(v)
		return nil
	})
	return
}

func (mbox *Mailbox) FromSeqNum(seqNum uint32) (apiID string, err error) {
	err = mbox.u.db.View(func(tx *bolt.Tx) error {
		b, err := mbox.bucket(tx)
		if err != nil {
			return err
		}

		c := b.Cursor()
		var n uint32 = 1
		for k, v := c.First(); k != nil; k, v = c.Next() {
			if seqNum == n {
				apiID = string(v)
				return nil
			}
			n++
		}

		return ErrNotFound
	})
	return
}

func (mbox *Mailbox) FromApiID(apiID string) (seqNum uint32, uid uint32, err error) {
	err = mbox.u.db.View(func(tx *bolt.Tx) error {
		b, err := mbox.bucket(tx)
		if err != nil {
			return err
		}

		want := []byte(apiID)
		c := b.Cursor()
		var n uint32 = 1
		for k, v := c.First(); k != nil; k, v = c.Next() {
			if bytes.Equal(v, want) {
				seqNum = n
				uid = unserializeUID(k)
				return nil
			}
			n++
		}

		return ErrNotFound
	})
	return
}

func (mbox *Mailbox) ForEach(f func(seqNum, uid uint32, apiID string) error) error {
	return mbox.u.db.View(func(tx *bolt.Tx) error {
		b, err := mbox.bucket(tx)
		if err != nil {
			return err
		}

		c := b.Cursor()
		var n uint32 = 1
		for k, v := c.First(); k != nil; k, v = c.Next() {
			if err := f(n, unserializeUID(k), string(v)); err != nil {
				return err
			}
			n++
		}

		return nil
	})
}

func (mbox *Mailbox) Reset() error {
	return mbox.u.db.Update(func(tx *bolt.Tx) error {
		b := tx.Bucket(mailboxesBucket)
		if b == nil {
			return errors.New("cannot find mailboxes bucket")
		}

		meta, err := ensureMailboxMetadata(tx, mbox.labelID)
		if err != nil {
			return err
		}
		validity := unserializeUID(meta.Get(uidValidityKey))

		k := []byte(mbox.labelID)
		if err := b.DeleteBucket(k); err != nil && err != bolt.ErrBucketNotFound {
			return err
		}
		if _, err := b.CreateBucket(k); err != nil {
			return err
		}

		if err := meta.Put(uidValidityKey, serializeUID(validity+1)); err != nil {
			return err
		}
		return meta.Put(uidNextKey, serializeUID(1))
	})
}
