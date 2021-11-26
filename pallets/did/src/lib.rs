#![cfg_attr(not(feature = "std"), no_std)]

pub mod did;
pub mod structs;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

// Re-export did items so that they can be accessed from the crate namespace.
pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use crate::did::DidError;
	use crate::did::*;
	use crate::structs::*;
	use frame_support::pallet_prelude::*;
	use frame_support::traits::Time as MomentTime;
	use frame_system::pallet_prelude::*;
	use sp_io::hashing::blake2_256;
	use sp_runtime::traits::{IdentifyAccount, Member, Verify};
	use sp_std::vec::Vec;

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Because this pallet emits events, it depends on the runtime's definition of an event.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;
		type Public: IdentifyAccount<AccountId = Self::AccountId>;
		type Signature: Verify<Signer = Self::Public> + Member + Decode + Encode;
		type Time: MomentTime;
	}

	// Pallets use events to inform users when important changes are made.
	// Event documentation should end with an array that provides descriptive names for parameters.
	// https://docs.substrate.io/v3/runtime/events-and-errors
	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Event emitted when an attribute has been added. [who, did_account, name, value, validity]
		AttributeAdded(T::AccountId, T::AccountId, Vec<u8>, Vec<u8>, Option<T::BlockNumber>),
		/// Event emitted when an attribute is read successfully
		AttributeRead(Attribute<T::BlockNumber, <<T as Config>::Time as MomentTime>::Moment>),
		/// Event emitted when an attribute has been updated. [who, did_account, name, validity]
		AttributeUpdated(T::AccountId, T::AccountId, Vec<u8>, Vec<u8>, Option<T::BlockNumber>),
		/// Event emitted when an attribute has been deleted. [who, did_acount name]
		AttributeRemoved(T::AccountId, T::AccountId, Vec<u8>),
	}

	#[pallet::error]
	pub enum Error<T> {
		// Name is greater that 64
		AttributeNameExceedMax64,
		// Attribute already exist
		AttributeAlreadyExist,
		// Attribute creation failed
		AttributeCreationFailed,
		// Attribute creation failed
		AttributeUpdateFailed,
		// Attribute was not found
		AttributeNotFound,
	}

	impl<T: Config> Error<T> {
		fn dispatch_error(err: DidError) -> DispatchResult {
			match err {
				DidError::NotFound => return Err(Error::<T>::AttributeNotFound.into()),
				DidError::AlreadyExist => return Err(Error::<T>::AttributeAlreadyExist.into()),
				DidError::NameExceedMaxChar => {
					return Err(Error::<T>::AttributeNameExceedMax64.into())
				}
				DidError::FailedCreate => return Err(Error::<T>::AttributeCreationFailed.into()),
				DidError::FailedUpdate => return Err(Error::<T>::AttributeCreationFailed.into()),
			}
		}
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	#[pallet::getter(fn attribute_of)]
	pub(super) type AttributeStore<T: Config> = StorageMap<
		_,
		Blake2_128Concat,
		(T::AccountId, [u8; 32]),
		Attribute<T::BlockNumber, <<T as Config>::Time as MomentTime>::Moment>,
		ValueQuery,
	>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	// Dispatchable functions allow users to interact with the pallet and invoke state changes.
	// These functions materialize as "extrinsics", which are often compared to transactions.
	// Dispatchable functions must be annotated with a weight and must return a DispatchResult.
	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Creates a new attribute as part of a DID
		/// with optional validity
		#[pallet::weight(1_000)]
		pub fn add_attribute(
			origin: OriginFor<T>,
			did_account: T::AccountId,
			name: Vec<u8>,
			value: Vec<u8>,
			valid_for: Option<T::BlockNumber>,
		) -> DispatchResult {
			// Check that an extrinsic was signed and get the signer
			// This fn returns an error if the extrinsic is not signed
			// https://docs.substrate.io/v3/runtime/origins
			let sender = ensure_signed(origin)?;

			// Verify that the name len is 64 max
			ensure!(name.len() <= 64, Error::<T>::AttributeNameExceedMax64);

			match Self::create(&sender, &did_account, &name, &value, valid_for) {
				Ok(()) => {
					Self::deposit_event(Event::AttributeAdded(
						sender,
						did_account,
						name,
						value,
						valid_for,
					));
				}
				Err(e) => return Error::<T>::dispatch_error(e),
			};

			Ok(())
		}

		/// Update an existing attribute of a DID
		/// with optional validity
		#[pallet::weight(1_000)]
		pub fn update_attribute(
			origin: OriginFor<T>,
			did_account: T::AccountId,
			name: Vec<u8>,
			value: Vec<u8>,
			valid_for: Option<T::BlockNumber>,
		) -> DispatchResult {
			// Check that an extrinsic was signed and get the signer
			// This fn returns an error if the extrinsic is not signed
			// https://docs.substrate.io/v3/runtime/origins
			let sender = ensure_signed(origin)?;

			// Verify that the name len is 64 max
			ensure!(name.len() <= 64, Error::<T>::AttributeNameExceedMax64);

			match Self::update(&sender, &did_account, &name, &value, valid_for) {
				Ok(()) => {
					Self::deposit_event(Event::AttributeUpdated(
						sender,
						did_account,
						name,
						value,
						valid_for,
					));
				}
				Err(e) => return Error::<T>::dispatch_error(e),
			};
			Ok(())
		}

		/// Read did attribute
		#[pallet::weight(1_000)]
		pub fn read_attribute(
			origin: OriginFor<T>,
			owner: T::AccountId,
			did_account: T::AccountId,
			name: Vec<u8>,
		) -> DispatchResult {
			// Check that an extrinsic was signed and get the signer
			// This fn returns an error if the extrinsic is not signed
			// https://docs.substrate.io/v3/runtime/origins
			ensure_signed(origin)?;

			let attribute = Self::read(&owner, &did_account, &name);
			match attribute {
				Some(attribute) => {
					Self::deposit_event(Event::AttributeRead(attribute));
				}
				None => return Err(Error::<T>::AttributeNotFound.into()),
			}
			Ok(())
		}

		/// Delete an existing attribute of a DID
		#[pallet::weight(1_000)]
		pub fn remove_attribute(
			origin: OriginFor<T>,
			did_account: T::AccountId,
			name: Vec<u8>,
		) -> DispatchResult {
			// Check that an extrinsic was signed and get the signer
			// This fn returns an error if the extrinsic is not signed
			// https://docs.substrate.io/v3/runtime/origins
			let sender = ensure_signed(origin)?;

			// Verify that the name len is 64 max
			ensure!(name.len() <= 64, Error::<T>::AttributeNameExceedMax64);

			match Self::delete(&sender, &did_account, &name) {
				Ok(()) => {
					// Get the block number from the FRAME system pallet
					Self::deposit_event(Event::AttributeRemoved(sender, did_account, name));
				}
				Err(e) => return Error::<T>::dispatch_error(e),
			};
			Ok(())
		}
	}

	// implements the Did trait to satisfied the required methods
	impl<T: Config> Did<T::AccountId, T::BlockNumber, <<T as Config>::Time as MomentTime>::Moment>
		for Pallet<T>
	{
		// Add new attribute to a did
		fn create(
			owner: &T::AccountId,
			did_account: &T::AccountId,
			name: &[u8],
			value: &[u8],
			valid_for: Option<T::BlockNumber>,
		) -> Result<(), DidError> {
			// Generate id for integrity check
			let id = (&owner, &did_account, name).using_encoded(blake2_256);

			// Check if attribute already exists
			if <AttributeStore<T>>::contains_key((&owner, &id)) {
				return Err(DidError::AlreadyExist);
			}

			let now_timestamp = T::Time::now();
			let validity: T::BlockNumber = match valid_for {
				Some(blocks) => {
					let now_block_number = <frame_system::Pallet<T>>::block_number();
					now_block_number + blocks
				}
				None => u32::max_value().into(),
			};

			let new_attribute = Attribute {
				name: (&name).to_vec(),
				value: (&value).to_vec(),
				validity,
				created: now_timestamp,
			};

			<AttributeStore<T>>::insert((&owner, &id), new_attribute);

			Ok(())
		}

		// Update existing attribute on a did
		fn update(
			owner: &T::AccountId,
			did_address: &T::AccountId,
			name: &[u8],
			value: &[u8],
			valid_for: Option<T::BlockNumber>,
		) -> Result<(), DidError> {
			let validity: T::BlockNumber = match valid_for {
				Some(blocks) => {
					let now_block_number = <frame_system::Pallet<T>>::block_number();
					now_block_number + blocks
				}
				None => u32::max_value().into(),
			};

			// Get attribute
			let attribute = Self::read(owner, did_address, name);

			match attribute {
				Some(mut attr) => {
					let id = (&owner, name, did_address).using_encoded(blake2_256);
					attr.value = (&value).to_vec();
					attr.validity = validity;

					<AttributeStore<T>>::mutate((&owner, &id), |a| *a = attr);
					Ok(())
				}
				None => Err(DidError::NotFound),
			}
		}

		// Fetch an attribute from a did
		fn read(
			owner: &T::AccountId,
			did_address: &T::AccountId,
			name: &[u8],
		) -> Option<Attribute<T::BlockNumber, <<T as Config>::Time as MomentTime>::Moment>> {
			let id = (&owner, did_address, name).using_encoded(blake2_256);

			if <AttributeStore<T>>::contains_key((&owner, &id)) {
				return Some(Self::attribute_of((&owner, &id)));
			}
			None
		}

		// Delete an attribute from a did
		fn delete(
			owner: &T::AccountId,
			did_address: &T::AccountId,
			name: &[u8],
		) -> Result<(), DidError> {
			let id = (&owner, did_address, name).using_encoded(blake2_256);

			if !<AttributeStore<T>>::contains_key((&owner, &id)) {
				return Err(DidError::NotFound);
			}
			<AttributeStore<T>>::remove((&owner, &id));
			Ok(())
		}
	}
}
